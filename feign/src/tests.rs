#[test]
fn test_host_round() {
    use crate::{Host, HostRound};
    let host_round =
        HostRound::new(vec!["a".to_string(), "b".to_string(), "c".to_string()]).unwrap();
    assert_eq!(host_round.host(), "a");
    assert_eq!(host_round.host(), "b");
    assert_eq!(host_round.host(), "c");
    assert_eq!(host_round.host(), "a");
    assert_eq!(host_round.host(), "b");
    assert_eq!(host_round.host(), "c");
}
