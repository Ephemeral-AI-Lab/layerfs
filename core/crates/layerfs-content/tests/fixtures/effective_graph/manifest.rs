// Generated independent test fixtures; see generate.py and vectors.json.
pub struct Case { pub name: &'static str, pub fresh: bool, pub root: u64, pub vertices: &'static [u64], pub edges: &'static [(u64,u64)], pub seeds: &'static [(u64,u64)], pub declared: &'static [u64], pub excluded: &'static [u64], pub expected: &'static str }
pub const CASES: &[Case] = &[
Case{name:"no_seed_old_cycle",fresh:false,root:1,vertices:&[1,2,3],edges:&[(2,3),(3,2)],seeds:&[],declared:&[],excluded:&[],expected:"pass"},
Case{name:"isolated_selected_child",fresh:false,root:1,vertices:&[1,2],edges:&[(1,2)],seeds:&[(1,2)],declared:&[],excluded:&[],expected:"pass"},
Case{name:"selected_self_loop",fresh:false,root:1,vertices:&[1,2],edges:&[(2,2)],seeds:&[(2,2)],declared:&[],excluded:&[],expected:"cycle"},
Case{name:"old_descendant_cycle_without_seed_return",fresh:false,root:1,vertices:&[1,2,3,4],edges:&[(1,2),(2,3),(3,4),(4,3)],seeds:&[(1,2)],declared:&[],excluded:&[],expected:"pass"},
Case{name:"selected_descendant_in_old_cycle",fresh:false,root:1,vertices:&[1,2,3,4],edges:&[(1,2),(2,3),(3,4),(4,3)],seeds:&[(3,4)],declared:&[],excluded:&[],expected:"cycle"},
Case{name:"selected_child_reaches_parent",fresh:false,root:1,vertices:&[1,2,3],edges:&[(1,2),(2,3),(3,1)],seeds:&[(1,2)],declared:&[],excluded:&[],expected:"cycle"},
Case{name:"disconnected_selected_component_cycle",fresh:false,root:1,vertices:&[1,2,9],edges:&[(9,2),(2,9)],seeds:&[(9,2)],declared:&[],excluded:&[],expected:"cycle"},
Case{name:"overlapping_seed_closures",fresh:false,root:1,vertices:&[1,2,3,4,5],edges:&[(1,2),(2,3),(3,4),(4,5)],seeds:&[(1,2),(2,3),(3,4)],declared:&[],excluded:&[],expected:"pass"},
Case{name:"final_batch_two_edge_cycle",fresh:false,root:1,vertices:&[1,2,3],edges:&[(1,2),(2,3),(3,2)],seeds:&[(1,2),(2,3),(3,2)],declared:&[],excluded:&[],expected:"cycle"},
Case{name:"acyclic_seed_plus_cyclic_seed",fresh:false,root:1,vertices:&[1,2,3,4,5,6],edges:&[(1,2),(2,3),(4,5),(5,6),(6,5)],seeds:&[(1,2),(4,5)],declared:&[],excluded:&[],expected:"cycle"},
Case{name:"diamond_old_alias_dag",fresh:false,root:1,vertices:&[1,2,3,4,5],edges:&[(1,2),(2,3),(2,4),(3,5),(4,5)],seeds:&[(1,2)],declared:&[],excluded:&[],expected:"pass"},
Case{name:"empty_fresh_root",fresh:true,root:1,vertices:&[1],edges:&[],seeds:&[],declared:&[1],excluded:&[],expected:"pass"},
Case{name:"fresh_chain",fresh:true,root:1,vertices:&[1,2,3,4],edges:&[(1,2),(2,3),(3,4)],seeds:&[],declared:&[1,2,3,4],excluded:&[],expected:"pass"},
Case{name:"fresh_disconnected_directory",fresh:true,root:1,vertices:&[1,2],edges:&[],seeds:&[],declared:&[1,2],excluded:&[],expected:"cycle"},
Case{name:"fresh_disconnected_cycle",fresh:true,root:1,vertices:&[1,2,3],edges:&[(2,3),(3,2)],seeds:&[],declared:&[1,2,3],excluded:&[],expected:"cycle"},
Case{name:"fresh_excluded_parent_does_not_reach_child",fresh:true,root:1,vertices:&[1,2,3],edges:&[(2,3)],seeds:&[],declared:&[1,2,3],excluded:&[2],expected:"cycle"},
Case{name:"fresh_excluded_component",fresh:true,root:1,vertices:&[1,2,3],edges:&[(2,3)],seeds:&[],declared:&[1,2,3],excluded:&[2,3],expected:"pass"},
Case{name:"fresh_reachable_multiple_parents",fresh:true,root:1,vertices:&[1,2,3,4],edges:&[(1,2),(1,3),(2,4),(3,4)],seeds:&[],declared:&[1,2,3,4],excluded:&[],expected:"multiple_parents"},
Case{name:"fresh_repeated_parent_binding",fresh:true,root:1,vertices:&[1,2],edges:&[(1,2),(1,2)],seeds:&[],declared:&[1,2],excluded:&[],expected:"multiple_parents"},
Case{name:"maximum_serial_selected_cycle",fresh:false,root:1,vertices:&[1,9223372036854775806,9223372036854775807],edges:&[(1,9223372036854775806),(9223372036854775806,9223372036854775807),(9223372036854775807,9223372036854775806)],seeds:&[(1,9223372036854775806),(9223372036854775806,9223372036854775807)],declared:&[],excluded:&[],expected:"cycle"},
];
