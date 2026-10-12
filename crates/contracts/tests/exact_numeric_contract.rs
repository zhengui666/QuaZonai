use contracts::DecimalValue;
use std::cmp::Ordering;

#[test]
fn observable_metric_comparison_preserves_exact_decimal_thresholds() {
    let exact: DecimalValue = "0.1".parse().unwrap();
    let greater: DecimalValue = "0.10000000000000001".parse().unwrap();
    assert_eq!(exact.compare_metric(0.1).unwrap(), Ordering::Equal);
    assert_eq!(greater.compare_metric(0.1).unwrap(), Ordering::Less);
    assert_eq!(
        "-0.10000000000000001"
            .parse::<DecimalValue>()
            .unwrap()
            .compare_metric(-0.1)
            .unwrap(),
        Ordering::Greater
    );
    assert_eq!(
        "0".parse::<DecimalValue>()
            .unwrap()
            .compare_metric(f64::MIN_POSITIVE)
            .unwrap(),
        Ordering::Greater
    );
    assert_eq!(exact.compare_metric(f64::MAX).unwrap(), Ordering::Greater);
    for number in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(exact.compare_metric(number).is_err());
    }
}
