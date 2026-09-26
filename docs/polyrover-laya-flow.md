# Flujo completo de Polyrover con Laya

## Resumen

Polyrover es el backend de decisiones que consume Arenaton. Arenaton no consulta Laya directamente: sólo consulta la API HTTP de Polyrover.

```text
Arenaton
   │
   │ GET /api/v1/decisions/{market_slug}
   │ POST /api/v1/decisions/{market_slug} con {}
   ▼
Polyrover Decision API
   │
   ├── PostgreSQL: cache, jobs, límites y reportes
   ├── Gamma/Polymarket: mercado, reglas y precios
   ├── Google News: búsqueda y fuentes
   └── Laya local: evaluación semántica
          │
          ▼
     Reporte advisory-only
```

Laya es el evaluador predeterminado. TypeSafe sólo permanece como compatibilidad heredada cuando se selecciona explícitamente.

## 1. Arranque local

El flujo local se inicia con:

```bash
bash scripts/serve-local.sh
```

El script:

1. Inicia PostgreSQL.
2. Compila Polyrover con `--features server`.
3. Levanta la API en `127.0.0.1:8787`.
4. Habilita generación.
5. Configura los orígenes web locales de Arenaton.
6. Aplica un límite diario configurable, cuyo valor local predeterminado es 10.

La configuración activa del backend utiliza:

```text
POLYROVER_AI_PROVIDER=laya
LAYA_BASE_URL=http://127.0.0.1:8000
POLYROVER_DATABASE_URL=<configuración local del servidor>
```

Laya se ejecuta separadamente en:

```text
http://127.0.0.1:8000
```

con el modelo local `typed-decisions` y dispositivo CPU en el entorno actual.

El servidor Polyrover no ejecuta órdenes, no firma transacciones y no mueve fondos.

## 2. Inicialización de Polyrover

En `src/cli/serve.rs`, Polyrover:

1. Lee y valida la dirección de escucha.
2. Valida los orígenes CORS.
3. Lee `POLYROVER_DATABASE_URL` desde el entorno o `.env` sin ejecutar el archivo como shell.
4. Conecta con PostgreSQL.
5. Crea o verifica las tablas requeridas.
6. Importa reportes legacy si existen.
7. Configura el límite diario de generaciones.
8. Registra el generador de decisiones.
9. Abre el listener HTTP.

Configuración local habitual:

```text
bind: 127.0.0.1:8787
daily generation limit: 10
generation timeout: 20 minutos
```

## 3. Consulta de Arenaton

Arenaton consulta un mercado específico mediante su slug:

```http
GET /api/v1/decisions/{market_slug}
```

Esta operación:

- sólo consulta PostgreSQL;
- no llama a Laya;
- no busca noticias;
- no consulta Gamma;
- no consume el límite diario;
- no inicia una generación.

Los estados posibles son:

```text
missing
running
ready
failed
```

## 4. Solicitud explícita de generación

Cuando el usuario pulsa el botón de análisis, Arenaton envía:

```http
POST /api/v1/decisions/{market_slug}
Content-Type: application/json

{}
```

Polyrover valida:

- el slug;
- que el body sea exactamente `{}`;
- que la generación esté habilitada;
- que no se haya alcanzado el límite diario;
- que no haya otro job ejecutándose;
- que no exista una investigación vigente reutilizable.

Respuestas principales:

| Código | Significado |
|---|---|
| `200` | Hay un reporte vigente en cache. |
| `202` | El job fue aceptado o ya está corriendo. |
| `403` | La generación está deshabilitada o el mercado no está permitido. |
| `429` | Hay otro job activo o se alcanzó el cooldown/límite. |
| `503` | PostgreSQL no está disponible. |

`retry_after_seconds: 3` significa que el cliente debe volver a consultar en tres segundos. No significa que la generación termine en tres segundos.

## 5. Reserva del job en PostgreSQL

Antes de llamar a Laya, PostgreSQL registra la generación.

Se guardan:

- slug;
- fecha de inicio;
- estado;
- lease del job;
- fecha de retry;
- fecha de finalización;
- resultado o fallo;
- límite diario compartido.

Esto evita:

- dos generaciones simultáneas del mismo mercado;
- doble consumo por clicks repetidos;
- saltarse los límites reiniciando el proceso;
- que distintas instancias generen simultáneamente.

Por eso Arenaton puede recibir:

```json
{
  "status": "running",
  "can_generate": false,
  "retry_after_seconds": 3
}
```

## 6. Obtención y validación del mercado

Polyrover obtiene desde Gamma/Polymarket:

- pregunta exacta;
- market ID;
- fecha límite;
- reglas de resolución;
- fuente oficial de resolución;
- estado del mercado;
- tokens YES/NO;
- información de aceptación de órdenes.

Antes de realizar la investigación se verifica si el mercado es elegible.

Si el mercado está cerrado, archivado, expirado, inactivo o no acepta órdenes, el flujo termina con:

```text
action = wait
```

sin realizar la investigación completa.

## 7. Selección de Laya

En `src/cli/typesafe.rs` se conserva una interfaz interna común para el evaluador, pero el proveedor predeterminado es Laya:

```rust
POLYROVER_AI_PROVIDER=laya
```

Cuando se selecciona Laya, Polyrover:

1. Cambia el modelo heredado `jev-latest` por `typed-decisions` si no se especifica otro.
2. Usa `LAYA_BASE_URL` o `http://127.0.0.1:8000`.
3. Crea un cliente local sin bearer token.
4. Envía las solicitudes a:

```http
POST http://127.0.0.1:8000/v1/systemone
```

Laya no recibe credenciales de Polymarket ni de wallet.

## 8. Búsqueda de noticias

Polyrover construye una consulta exacta con la pregunta del mercado y usa Google News RSS.

El flujo de recolección es:

1. Buscar la pregunta exacta.
2. Mantener el locale configurado.
3. Obtener el snapshot de resultados.
4. Resolver los enlaces de Google hacia el publisher original.
5. Descargar las páginas públicas.
6. Extraer el texto.
7. Registrar artículos bloqueados o inaccesibles.
8. Detectar duplicados mediante hash del contenido.

La descarga de artículos utiliza hasta tres artículos en vuelo simultáneamente.

Google News es un snapshot acotado, no una búsqueda exhaustiva de Internet.

## 9. Clasificación de fechas

Polyrover usa primero la fecha `pubDate` del RSS.

Si falta, intenta obtener la fecha desde metadatos del publisher:

- `article:published_time`;
- `article:published`;
- `date`;
- `pubdate`;
- `publish-date`;
- `datePublished`;
- `time[datetime]`.

Se aceptan formatos:

- RFC3339;
- RFC2822;
- `YYYY-MM-DD`.

Después se compara la fecha con `max_age_days`.

El valor predeterminado actual es:

```text
max_age_days = 7
```

Una fuente se excluye de la evidencia final si:

- no tiene fecha;
- es demasiado antigua;
- está fechada en el futuro;
- no pudo extraerse;
- está incompleta;
- no es relevante para el evento exacto.

Que un artículo sea leído no significa que pueda utilizarse como evidencia final.

### Fallback de búsqueda por evidencia reciente

Para una investigación con ventana inicial de 7 días, Polyrover no se queda
con una sola consulta si encuentra menos de tres artículos extraídos y
recientes. Amplía de forma controlada la búsqueda a 14 y después a 30 días.
En cada ventana ejecuta la consulta temporal y una consulta temática adicional:

- mercados políticos: cambio de régimen, caída del gobierno y transición
  política;
- otros mercados: comunicados oficiales, pronósticos y análisis.

La ventana efectiva queda registrada en el reporte. Este fallback aumenta la
posibilidad de encontrar evidencia trazable, pero no relaja los quality gates:
fuentes sin fecha, inaccesibles, irrelevantes o sin diversidad independiente
siguen excluidas y la acción sigue siendo `wait` si la evidencia no es
defendible.

## 10. Evaluación de artículos con Laya

Cada artículo se divide en chunks de aproximadamente 12.000 bytes.

Cada chunk se envía a Laya para evaluar:

```text
relevance
direction
evidence_kind
```

### Relevancia

Determina si el texto corresponde al evento exacto.

Para mercados deportivos, se consideran potencialmente relevantes:

- previas de temporada;
- análisis de la carrera por el título;
- pronósticos expertos;
- proyecciones cuantitativas;
- análisis del equipo correcto;
- análisis de la competición y temporada correctas.

Se rechazan referencias a:

- otro año;
- otra competición;
- otro club;
- otra categoría;
- otra rama masculina o femenina;
- una mención incidental.

### Dirección

Puede ser:

```text
supports_yes
supports_no
mixed
neutral
irrelevant
```

### Tipo de evidencia

Puede ser:

```text
reported_facts
forecast_opinion
mixed
unclear
```

El texto del artículo se trata como dato no confiable, no como instrucciones para Laya.

## 11. Selección de evidencia

Polyrover selecciona únicamente artículos que cumplan simultáneamente:

```text
texto extraído completamente
fecha vigente
relevancia suficiente
texto disponible
publisher identificable
```

También calcula:

- artículos encontrados;
- artículos extraídos;
- artículos inaccesibles;
- artículos evaluados;
- fallos de evaluación;
- artículos antiguos o sin fecha;
- artículos relevantes;
- publishers independientes;
- evidencia a favor;
- evidencia en contra;
- evidencia mixta.

Para superar el filtro de diversidad se requieren al menos dos publishers relevantes.

Por ese motivo es válido observar un resultado como:

```text
77 artículos evaluados
100 artículos encontrados
23 artículos inaccesibles
0 fuentes usadas en la síntesis
```

Los 77 artículos fueron procesados, pero ninguno superó simultáneamente los filtros de fecha, relevancia y evidencia defendible.

## 12. Síntesis final con Laya

Las fuentes seleccionadas se empaquetan en batches respetando el límite de contexto.

Laya recibe:

- pregunta del mercado;
- reglas de resolución;
- fecha límite;
- fuente oficial de resolución;
- cobertura de la investigación;
- textos de las fuentes seleccionadas;
- fechas y publishers.

Laya produce:

```text
outlook
reason
probability_band
basis
```

El resultado puede ser:

```text
yes
no
uncertain
insufficient_evidence
```

Polyrover no fabrica probabilidades a partir de:

- cantidad de artículos;
- cantidad de opiniones;
- precios del mercado;
- memoria del modelo;
- conocimiento externo;
- titulares repetidos.

Si no existe una base numérica defendible:

```text
predicted_outcome = insufficient_evidence
```

## 13. Revisión de reglas

Polyrover realiza una evaluación separada de las reglas del mercado mediante `review_market`.

Se verifica:

- si la pregunta es resoluble;
- si la fecha está clara;
- si la fuente de resolución está identificada;
- si las reglas son suficientemente precisas;
- si el evento corresponde al mercado.

Si la revisión falla, se añade:

```text
resolution_rules_need_review
```

Esto no significa que Laya esté caído. Significa que el mercado no está suficientemente definido para una recomendación operable.

## 14. Actualización del mercado y precios

Los precios se consultan al final, después de la investigación.

Polyrover vuelve a obtener:

- estado actual del mercado;
- order book YES;
- order book NO;
- asks;
- liquidez;
- comisiones;
- timestamps de las cotizaciones.

Esto evita que las cotizaciones caduquen mientras se investiga.

Las decisiones con precios son válidas como máximo durante 120 segundos.

Si las cotizaciones caducan:

```text
action = wait
```

La investigación histórica se conserva, pero no se reutiliza una cotización vieja como recomendación actual.

## 15. Política de decisión

La política predeterminada utiliza:

```text
shares: 10
min_edge: 0.05
model_risk_margin: 0.10
slippage_reserve: 0.01
min_forecast_confidence: 0.65
min_evaluated_fraction: 0.60
```

La acción puede ser:

```text
buy_yes
buy_no
wait
```

La acción es siempre advisory-only.

Polyrover no:

- crea órdenes;
- firma transacciones;
- usa wallets;
- envía operaciones;
- mueve fondos;
- prepara tickets de compra.

## 16. Persistencia

Cuando finaliza la evaluación, el reporte se guarda en PostgreSQL.

Se conservan:

- investigación;
- cobertura;
- fuentes;
- forecast;
- razones;
- limitaciones;
- precios usados;
- timestamps;
- acción histórica;
- expiración.

La investigación se reutiliza durante 24 horas desde su generación.

El precio no se conserva como válido durante 24 horas: sólo la investigación se cachea durante ese periodo.

La expiración no elimina el historial ni inicia una tarea automáticamente. Una nueva investigación requiere un POST explícito posterior.

## 17. Respuesta final a Arenaton

Polyrover proyecta el reporte a un envelope público reducido.

No devuelve:

- cuerpos completos de artículos;
- credenciales;
- tokens;
- secretos;
- instrucciones internas del evaluador.

Devuelve campos como:

```json
{
  "schema_version": "polyrover_decision_v1",
  "slug": "market-slug",
  "status": "ready",
  "can_generate": false,
  "generation_enabled": true,
  "cache_hit": true,
  "research_ttl_seconds": 86400,
  "data": {
    "action": "wait",
    "reported_action": "wait",
    "predicted_outcome": "insufficient_evidence",
    "forecast_status": "no_eligible_evidence",
    "evidence_used": 0,
    "stale_sources": 87,
    "coverage": {
      "discovered": 100,
      "evaluated": 77,
      "unavailable": 23
    }
  }
}
```

## 18. Interpretación en Arenaton

Arenaton debe:

1. Consultar el estado con `GET`.
2. Si recibe `running`, esperar `retry_after_seconds`.
3. Volver a consultar.
4. Mostrar el reporte cuando reciba `ready`.
5. Mostrar `wait` si los precios caducaron.
6. No solicitar precios nuevos mediante `GET`.
7. No llamar directamente a Laya.
8. No conocer ni configurar las credenciales del evaluador.

## Flujo completo resumido

```text
Usuario pulsa Escanear
        │
        ▼
Arenaton POST /api/v1/decisions/{slug}
        │
        ▼
Polyrover valida slug, permisos, límites y cache
        │
        ▼
PostgreSQL reserva el job
        │
        ▼
Polyrover obtiene mercado y reglas
        │
        ▼
Comprueba elegibilidad
        │
        ▼
Google News devuelve fuentes
        │
        ▼
Polyrover descarga, extrae, fecha y deduplica
        │
        ▼
Polyrover divide artículos en chunks
        │
        ▼
Laya evalúa relevancia, dirección y tipo de evidencia
        │
        ▼
Polyrover filtra fuentes actuales y relevantes
        │
        ▼
Laya sintetiza outlook, razón y base
        │
        ▼
Polyrover revisa reglas y actualiza precios
        │
        ▼
Aplica la política advisory
        │
        ▼
Guarda el reporte en PostgreSQL
        │
        ▼
Arenaton hace polling mediante GET
        │
        ▼
Polyrover devuelve ready + data
```

## Caso Arsenal

El caso de Arsenal terminó con:

```text
77 artículos evaluados
23 inaccesibles
87 antiguos o sin fecha válida
0 fuentes relevantes usadas en la síntesis
0 publishers relevantes
predicted_outcome = insufficient_evidence
action = wait
```

Esto significa que el flujo funcionó, pero la evidencia disponible no fue suficiente para producir un pronóstico verificable. No representa un fallo de transporte, de Arenaton ni necesariamente de Laya.

## 19. Qué recibe Laya

Polyrover utiliza el cliente local de Laya para enviar solicitudes estructuradas. La solicitud contiene un modelo, un estado (`state`) y un conjunto de preguntas tipadas. El texto de los artículos se incluye como datos dentro del estado; no se envía como instrucciones ejecutables.

Laya participa en tres etapas distintas.

### 19.1 Revisión del mercado

En `review_market`, Laya recibe los datos necesarios para revisar si el mercado es resoluble, incluyendo:

```json
{
  "market": {
    "id": "...",
    "slug": "...",
    "question": "...",
    "description": "...",
    "end_date": "...",
    "resolution_source": "...",
    "evidence": "...",
    "outcomes": ["Yes", "No"]
  }
}
```

La representación exacta puede incluir campos adicionales del objeto público del mercado. El modelo se especifica en el envelope de la solicitud y `min_confidence` se utiliza localmente para validar la respuesta; no forma parte del estado de evidencia enviado a Laya. Esta evaluación sirve para revisar la pregunta y las reglas; no es todavía un pronóstico ni una recomendación de compra.

### 19.2 Evaluación de cada artículo

Cada artículo se divide en uno o más chunks. Para cada chunk, Laya recibe un estado conceptualmente equivalente a:

```json
{
  "market_question": "¿Ganará Arsenal el campeonato de la Premier League 2026-27?",
  "article": {
    "id": "...",
    "title": "...",
    "url": "...",
    "published_at": "2026-09-20T...Z",
    "retrieved_at": "2026-09-26T09:20:35Z",
    "text": "Contenido extraído del chunk...",
    "chunk_index": 0,
    "chunks_total": 2,
    "publisher_completeness_verified": false
  }
}
```

Las preguntas de esta etapa piden clasificar:

```text
relevance
direction
evidence_kind
```

Laya debe decidir si el chunk:

- corresponde al evento exacto;
- apoya YES, apoya NO, es mixto, neutral o irrelevante;
- contiene hechos reportados, un pronóstico, una mezcla o evidencia poco clara.

El texto del artículo se considera contenido no confiable. No puede cambiar las reglas del sistema, ordenar acciones ni modificar la política de Polyrover.

### 19.3 Síntesis del pronóstico

Después de filtrar las fuentes, Polyrover construye una o más solicitudes de síntesis. Laya recibe:

```json
{
  "question": "¿Ganará Arsenal el campeonato de la Premier League 2026-27?",
  "as_of": "2026-09-26T09:20:35Z",
  "resolution_rules": "...",
  "resolution_source": "...",
  "deadline": "2027-05-...",
  "coverage": {
    "discovered": 100,
    "evaluated": 77,
    "unavailable": 23,
    "relevant_articles": 0,
    "relevant_publisher_hosts": 0
  },
  "articles": [
    {
      "id": "...",
      "title": "...",
      "url": "...",
      "published_at": "...",
      "text": "Texto de evidencia seleccionado...",
      "chunk_index": 0,
      "chunks_total": 1
    }
  ]
}
```

En esta etapa las preguntas tipadas son:

```text
outlook
reason
probability_band
basis
```

Las respuestas posibles distinguen entre:

- `yes`;
- `no`;
- `uncertain`;
- `insufficient_evidence`;
- hechos observados;
- pronósticos respaldados;
- evidencia contradictoria;
- contexto que no establece una dirección;
- fuentes insuficientes;
- previsiones cuantitativas;
- tasas base cuantitativas;
- base insuficiente para una probabilidad.

### 19.4 Lo que Laya no recibe

En estas evaluaciones Laya no recibe:

- claves de `TYPESAFE_API_KEY`;
- credenciales de PostgreSQL;
- claves privadas;
- wallets;
- seed phrases;
- órdenes de compra;
- permisos para enviar órdenes;
- secretos de Arenaton;
- instrucciones para ejecutar código;
- una autoridad para modificar las reglas del mercado.

La síntesis tampoco utiliza precios del mercado para construir la probabilidad. Los precios y order books se consultan después, en una etapa separada, para comprobar si una decisión advisory tendría margen y si sus cotizaciones siguen vigentes.

### 19.5 Qué devuelve Laya a Polyrover

Laya devuelve respuestas estructuradas que Polyrover valida antes de continuar. Polyrover no acepta una respuesta como evidencia automáticamente: comprueba el esquema, la confianza mínima, los valores permitidos y la correspondencia con la pregunta.

Polyrover conserva en el reporte los resultados de evaluación y los metadatos, pero no persiste los cuerpos completos de los artículos como archivo de investigación público.

## 20. Ejemplo completo de una evaluación con Laya

Este ejemplo es ilustrativo: muestra el flujo y el formato de los datos, pero los valores, textos, probabilidades y timestamps no representan una evaluación real ni una resolución oficial.

### 20.1 Mercado de ejemplo

```text
Pregunta: Will Arsenal win the 2026-27 English Premier League (EPL) championship?
Slug: will-arsenal-win-the-2026-27-english-premier-league-epl-championship-20260701200428750
Fecha límite: 2027-05-31T23:59:59Z
Resultados: Yes / No
```

### 20.2 Flujo de preguntas y respuestas

Polyrover envía a Laya un `Request` con `model`, `state` y `questions`. Primero revisa el mercado:

```json
{
  "model": "typed-decisions",
  "state": {"market": {"id": "123456789", "slug": "will-arsenal-win-the-2026-27-english-premier-league-epl-championship-20260701200428750", "question": "Will Arsenal win the 2026-27 English Premier League (EPL) championship?", "description": "The market resolves YES if Arsenal wins the 2026-27 season under the official source.", "resolution_source": "Official competition source", "evidence": "", "end_date": "2027-05-31T23:59:59Z", "outcomes": ["Yes", "No"]}},
  "questions": {
    "category": {"type": "choice", "criteria": {"sports": "Sporting events", "other": "Other or insufficient"}},
    "resolution_clarity": {"type": "score", "criteria": ["Missing", "Ambiguous", "Mostly explicit", "Explicit and objective"]},
    "resolution_source_identified": {"type": "noul", "instructions": "Is the resolution source identified?"}
  }
}
```

Laya consulta `category`, `resolution_clarity` y `resolution_source_identified`, y puede devolver:

```json
{
  "model": "typed-decisions",
  "answers": {
    "category": {"type": "choice", "choice": "sports", "probabilities": {"sports": 0.99, "other": 0.01}, "confidence": 0.99},
    "resolution_clarity": {"type": "score", "score": 3.0, "probabilities": {"0": 0.01, "1": 0.02, "2": 0.12, "3": 0.85}, "confidence": 0.85, "legend": {"3": "Explicit and objective"}},
    "resolution_source_identified": {"type": "noul", "noul": 0.96}
  },
  "usage": {"input_tokens": 900, "output_tokens": 180}
}
```

Interpretación de Polyrover: el mercado es deportivo, sus reglas son suficientemente claras y la investigación puede continuar. Laya todavía no ha decidido quién ganará.

### 20.3 Evaluación de un artículo

Polyrover descarga un artículo, lo divide en chunks y envía a Laya:

```json
{
  "model": "typed-decisions",
  "state": {"market_question": "Will Arsenal win the 2026-27 English Premier League (EPL) championship?", "article": {"id": "article-001", "title": "Arsenal emerge as early title contenders", "url": "https://publisher.example/arsenal-title-contenders", "published_at": "2026-09-20T10:00:00Z", "retrieved_at": "2026-09-26T09:20:35Z", "text": "Arsenal are among the early favorites after strengthening the squad...", "chunk_index": 0, "chunks_total": 1, "publisher_completeness_verified": false}},
  "questions": {
    "relevance": {"type": "noul", "instructions": "Is this evidence relevant to the exact event, competition, season and team?"},
    "direction": {"type": "choice", "criteria": {"supports_yes": "Favors YES", "supports_no": "Favors NO", "mixed": "Both directions", "neutral": "Context only", "irrelevant": "Wrong event"}},
    "evidence_kind": {"type": "choice", "criteria": {"reported_facts": "Attributed facts", "forecast_opinion": "Prediction or opinion", "mixed": "Both", "unclear": "Unclear"}}
  }
}
```

Laya consulta `relevance`, `direction` y `evidence_kind`, y puede responder:

```json
{
  "model": "typed-decisions",
  "answers": {
    "relevance": {"type": "noul", "noul": 0.94},
    "direction": {"type": "choice", "choice": "supports_yes", "probabilities": {"supports_yes": 0.82, "supports_no": 0.03, "mixed": 0.08, "neutral": 0.05, "irrelevant": 0.02}, "confidence": 0.82},
    "evidence_kind": {"type": "choice", "choice": "forecast_opinion", "probabilities": {"reported_facts": 0.12, "forecast_opinion": 0.78, "mixed": 0.08, "unclear": 0.02}, "confidence": 0.78}
  },
  "usage": {"input_tokens": 1500, "output_tokens": 260}
}
```

Polyrover decide que el artículo es relevante y favorece YES, pero es un pronóstico, no un hecho observado. Sólo lo incluirá si también pasa los filtros de fecha, extracción y diversidad de publishers.

### 20.4 Síntesis de la evidencia

Tras filtrar los artículos, Polyrover envía a Laya la pregunta, las reglas, la cobertura y los artículos seleccionados:

```json
{
  "model": "typed-decisions",
  "state": {
    "question": "Will Arsenal win the 2026-27 English Premier League (EPL) championship?",
    "as_of": "2026-09-26T09:20:35Z",
    "resolution_rules": "YES if Arsenal wins the specified season under the official source.",
    "resolution_source": "Official competition source",
    "deadline": "2027-05-31T23:59:59Z",
    "coverage": {"discovered": 100, "evaluated": 77, "unavailable": 23, "relevant_articles": 4, "relevant_publisher_hosts": 3},
    "articles": [{"id": "article-001", "title": "Arsenal emerge as early title contenders", "url": "https://publisher.example/arsenal-title-contenders", "published_at": "2026-09-20T10:00:00Z", "text": "Arsenal are among the early favorites...", "chunk_index": 0, "chunks_total": 1}]
  },
  "questions": {
    "outlook": {"type": "choice", "criteria": {"yes": "YES better supported", "no": "NO better supported", "uncertain": "Balanced or contradictory", "insufficient_evidence": "No meaningful evidence"}},
    "reason": {"type": "choice", "criteria": {"observed_facts": "Facts", "supported_forecast": "Expert forecasts", "conflicting_evidence": "Opposing evidence", "only_context": "Background", "insufficient_sources": "Insufficient sources"}},
    "probability_band": {"type": "choice", "criteria": {"p5": "50%-60%", "p8": "80%-90%", "insufficient": "No defensible numerical likelihood"}},
    "basis": {"type": "choice", "criteria": {"quantitative_forecasts": "Numerical forecasts", "quantitative_base_rates": "Base rates", "conflicting": "Conflicting quantitative sources", "insufficient": "Qualitative or insufficient"}}
  }
}
```

Laya consulta `outlook`, `reason`, `probability_band` y `basis`, y puede responder:

```json
{
  "model": "typed-decisions",
  "answers": {
    "outlook": {"type": "choice", "choice": "uncertain", "probabilities": {"yes": 0.32, "no": 0.08, "uncertain": 0.52, "insufficient_evidence": 0.08}, "confidence": 0.52},
    "reason": {"type": "choice", "choice": "conflicting_evidence", "probabilities": {"observed_facts": 0.10, "supported_forecast": 0.31, "conflicting_evidence": 0.49, "only_context": 0.07, "insufficient_sources": 0.03}, "confidence": 0.49},
    "probability_band": {"type": "choice", "choice": "insufficient", "probabilities": {"p5": 0.07, "p8": 0.02, "insufficient": 0.88}, "confidence": 0.88},
    "basis": {"type": "choice", "choice": "insufficient", "probabilities": {"quantitative_forecasts": 0.03, "quantitative_base_rates": 0.02, "conflicting": 0.10, "insufficient": 0.85}, "confidence": 0.85}
  },
  "usage": {"input_tokens": 4200, "output_tokens": 620}
}
```

### 20.5 Resultado final de Polyrover

Laya ha decidido semánticamente que la evidencia es contradictoria y que no hay base numérica defendible. Polyrover lo transforma en:

```json
{
  "predicted_outcome": "uncertain",
  "forecast_reason": "conflicting_evidence",
  "forecast_basis": "insufficient",
  "yes_interval": null,
  "classifier_confidence": 0.52
}
```

Después Polyrover consulta precios, liquidez y comisiones. Si las cotizaciones caducaron, no hay margen o no se alcanza la política de confianza, la respuesta operativa es:

```json
{
  "action": "wait",
  "reported_action": "wait",
  "orders_submitted": 0
}
```

### 20.6 Diferencia entre Laya y Polyrover

Laya resuelve clasificaciones semánticas:

```text
¿De qué categoría es el mercado?
¿Son claras sus reglas?
¿El artículo es relevante?
¿Favorece YES, NO o es mixto?
¿Es un hecho, una opinión o un pronóstico?
¿La evidencia permite una dirección?
¿Existe una base numérica para una probabilidad?
```

Polyrover resuelve controles operativos:

```text
¿El mercado está abierto?
¿Qué fuentes pasan los filtros?
¿Hay publishers independientes suficientes?
¿Hay margen tras riesgo, slippage y comisiones?
¿Las cotizaciones siguen vigentes?
¿La acción final es buy_yes, buy_no o wait?
```

Laya no resuelve oficialmente quién ganó la Premier League. La resolución oficial ocurre posteriormente mediante la fuente definida en las reglas del mercado. Polyrover sólo produce un pronóstico experimental y una acción advisory basada en la evidencia disponible.
