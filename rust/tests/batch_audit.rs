use kras_quick::auto_batch::{scope_includes_target, AutoBuilding, AutoTarget};
use kras_quick::quick_address::AutoBatchScope;

#[test]
fn full_scope_discovers_floors_without_capturing_an_arbitrary_first_room() {
    let target = AutoTarget::AggregateTotal {
        building: AutoBuilding { index: 1, value: "aggregate".into(), label: "A동 (집합)".into() },
    };
    assert!(!scope_includes_target(AutoBatchScope::Full, &target));
    assert!(!scope_includes_target(AutoBatchScope::FloorsOnly, &target));
}
