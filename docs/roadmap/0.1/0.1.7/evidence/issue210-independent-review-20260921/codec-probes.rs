use layerfs_bridge::{contract::*,adapters::native::protocol::{encode_response,decode_response}};
#[test]
fn review_minimal_legal_reply_pages_are_refused() {
    let mut stack=[1;17];stack[0]=0x31;let mut branch=[1;17];branch[0]=0x11;let mut layer=[1;33];layer[0]=0x32;
    let pages=[("branch",HistoryResult::Branches{continuation:vec![],records:vec![BranchWire{branch,stack,name:b"a".to_vec(),base_layer:layer,head_commit:None}]}),
        ("genesis",HistoryResult::Layers{continuation:vec![],records:vec![LayerWire{layer,stack,parent:None,root:[2;32],source_branch:None,source_commit:None}]})];
    for (name,page) in pages {let encoded=encode_response(&Response::History(Box::new(page))).unwrap();let e=decode_response(&encoded).unwrap_err();println!("{name}: encoded={} record={} decode={e:?}",encoded.len(),encoded.len()-6);assert_eq!(e.code,Code::Capacity);}
}
