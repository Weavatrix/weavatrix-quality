//! Application Surface Graph reach into declared CodeDelta flows.

use super::*;
use wvq_proof::scoped_code_delta;

#[test]
fn a_test_binding_with_a_graph_edge_maps_the_production_node() {
    let graph = json!({
        "nodes": [
            {
                "id": "symbol:src/widget.test.ts#renders",
                "span": {"file": "src/widget.test.ts", "start_line": 1, "end_line": 1}
            },
            {
                "id": "symbol:src/widget.ts#Widget",
                "span": {"file": "src/widget.ts", "start_line": 1, "end_line": 1}
            }
        ],
        "edges": [{
            "source": "symbol:src/widget.test.ts#renders",
            "target": "symbol:src/widget.ts#Widget"
        }]
    });
    let bindings = [TestBinding {
        path: "src/widget.test.ts".into(),
        runner: None,
        suite: None,
        case: None,
        obligations: BTreeSet::from(["export-usable".into()]),
        cost: 100,
        flake_penalty: 0,
    }];
    let flows = declared_code_flows("rev-head", &bindings, &graph);
    assert_eq!(flows.len(), 1);
    assert_eq!(flows[0].flow, "symbol:src/widget.ts#Widget");
    assert_eq!(flows[0].tests, ["src/widget.test.ts"]);
    assert_eq!(flows[0].proven_obligations, ["export-usable"]);

    let checkout = [wvq_domain::ObligationId::new("export-usable").unwrap()];
    let changed = BTreeSet::from(["symbol:src/widget.ts#Widget".into()]);
    let delta = scoped_code_delta(&checkout, &flows, &changed);
    assert!(delta.measured);
    assert!(delta.changed);
}
