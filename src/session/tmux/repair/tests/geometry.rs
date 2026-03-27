use super::super::geometry::{PaneLeftMetric, parse_pane_left_metrics, select_right_column_anchor};

#[test]
fn select_right_column_anchor_prefers_farthest_right_pane() {
    let metrics = vec![
        PaneLeftMetric {
            pane_id: String::from("%2"),
            left: 0,
        },
        PaneLeftMetric {
            pane_id: String::from("%1"),
            left: 53,
        },
        PaneLeftMetric {
            pane_id: String::from("%5"),
            left: 115,
        },
    ];

    assert_eq!(
        select_right_column_anchor("%1", &metrics),
        Some(String::from("%5"))
    );
}

#[test]
fn select_right_column_anchor_returns_none_without_right_column_candidate() {
    let metrics = vec![
        PaneLeftMetric {
            pane_id: String::from("%2"),
            left: 0,
        },
        PaneLeftMetric {
            pane_id: String::from("%1"),
            left: 53,
        },
    ];

    assert!(select_right_column_anchor("%1", &metrics).is_none());
}

#[test]
fn parse_pane_left_metrics_discards_malformed_rows() {
    let output = "%2|0\nmalformed\n%1|53\n%bad|not-a-number\n";

    assert_eq!(
        parse_pane_left_metrics(output),
        vec![
            PaneLeftMetric {
                pane_id: String::from("%2"),
                left: 0,
            },
            PaneLeftMetric {
                pane_id: String::from("%1"),
                left: 53,
            },
        ]
    );
}
