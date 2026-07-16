use polyrover::capabilities;

fn ids() -> Vec<String> {
    capabilities::all().into_iter().map(|cap| cap.id).collect()
}

#[test]
fn reporting_matches_compiled_features() {
    let ids = ids();
    assert_eq!(
        ids.contains(&"gamma.markets".into()),
        cfg!(feature = "public")
    );
    assert_eq!(
        ids.contains(&"websocket.user".into()),
        cfg!(feature = "authenticated")
    );
    assert_eq!(
        ids.contains(&"relayer.deposit_wallet".into()),
        cfg!(feature = "wallet")
    );
    assert_eq!(
        ids.contains(&"clob.trading".into()),
        cfg!(feature = "execution")
    );
    assert_eq!(
        ids.contains(&"bridge.funding".into()),
        cfg!(feature = "bridge")
    );
}

#[test]
fn reported_capabilities_stay_sorted() {
    let ids = ids();
    assert!(ids.windows(2).all(|pair| pair[0] <= pair[1]));
}
