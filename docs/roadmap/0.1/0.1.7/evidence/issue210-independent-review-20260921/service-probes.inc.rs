fn review_directory(parent:u16,name:&[u8])->ManifestEntry {
    ManifestEntry{parent,name:name.to_vec(),kind:2,mode:0o755,mtime_seconds:0,mtime_nanoseconds:0,content:None,target:vec![]}
}

#[test]
fn review_empty_child_directory_and_interleaved_parents() {
    let f=fixture("review-directory",ALL);
    let empty_child=vec![review_directory(0,b""),review_directory(0,b"a")];
    let r=call(&f.service,&f.peer,1,Operation::HistoryCommand(HistoryCommand::InitLayerStack{stack:[1;16],name:b"empty-child".to_vec(),scope_seed:[1;32],manifest:empty_child}));
    println!("empty child directory initialization: {r:?}");
    if let Ok(response)=r {let s=stack_wire(response);let l=f.catalog.layer(layerfs_history::LayerId::from_bytes(s.head_layer).unwrap()).unwrap().unwrap();
        let listed=call(&f.service,&f.peer,2,Operation::Inspect{root:*l.root.as_bytes(),query:Inspect::List{path:b"a".to_vec(),after:vec![],entries:128,bytes:16384}});
        println!("empty child directory listing: {listed:?}");assert!(listed.is_err());
    }
    let interleaved=vec![review_directory(0,b""),review_directory(0,b"a"),review_directory(1,b"x"),review_directory(0,b"b")];
    let r=call(&f.service,&f.peer,3,Operation::HistoryCommand(HistoryCommand::InitLayerStack{stack:[2;16],name:b"interleaved".to_vec(),scope_seed:[2;32],manifest:interleaved}));
    println!("legal interleaved parents: {r:?}");assert!(matches!(r,Err(Failure{code:Code::InvalidInput,..})));
}

#[test]
fn review_get_branch_accepts_missing_filesystem_root() {
    let f=fixture("review-missing-root",ALL);
    let missing=layerfs_content::ObjectId::for_bytes(b"no such content object");
    let s=f.catalog.initialize_layerstack(&layerfs_history::StackInitialization{stack:layerfs_history::LayerStackId::from_authority([5;16]),name:layerfs_history::HistoryName::new("missing").unwrap(),scope:scope_for_seed([5;32]).object(),profile:layerfs_content::filesystem::profile_id(),genesis_root:missing}).unwrap();
    let b=f.catalog.fork(&layerfs_history::ForkRequest{stack:s.id,branch:layerfs_history::BranchId::from_authority([6;16]),name:layerfs_history::HistoryName::new("b").unwrap(),source:layerfs_history::ForkSource::Layer(s.head_layer)}).unwrap();
    let r=call(&f.service,&f.peer,1,Operation::HistoryQuery(HistoryQuery::GetBranch{branch:b.branch.id.to_bytes()}));
    println!("GetBranch with absent C2 root succeeds: {}",r.is_ok());assert!(r.is_ok());
}
