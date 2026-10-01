use crate::{
    engine::Engine,
    fuse::SqlFs,
    metadata::Remote,
    objects::{Consumer, Reader},
    wire::Snapshot,
};
use layerfs_bridge::{
    adapters::native::{
        connection::{self, Peer},
        server,
    },
    contract::*,
};
use layerfs_content::filesystem::InodeScope;
use std::{
    net::TcpListener,
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
};
struct Live {
    name: Vec<u8>,
    incarnation: [u8; 32],
    path: PathBuf,
    engine: Arc<Mutex<Engine>>,
    session: Option<fuser::BackgroundSession>,
    reader: Reader,
    remote: Remote,
    base: Snapshot,
    generation: u64,
    pending: bool,
}
struct State {
    hello: SandboxHelloWire,
    live: Option<Live>,
}
fn bad(e: String) -> Failure {
    eprintln!("experimental daemon: {e}");
    Code::Io.into()
}
fn env(name: &str) -> Result<String, Failure> {
    std::env::var(name).map_err(|_| Code::InvalidInput.into())
}
fn key(name: &str) -> Result<[u8; 32], Failure> {
    layerfs_bridge::adapters::native::pipe::key(&env(name)?)
}
pub fn run() -> Result<(), Failure> {
    let private = key("LAYERFS_PRIVATE_KEY")?;
    let remote = Remote {
        endpoint: env("LAYERFS_ENDPOINT")?,
        selector: env("LAYERFS_SELECTOR")?
            .parse()
            .map_err(|_| Code::InvalidInput)?,
        private,
        server: key("LAYERFS_SERVER_KEY")?,
    };
    let mut peers = Vec::new();
    for entry in env("LAYERFS_CONTROL_PEERS")?.split(';') {
        let p: Vec<_> = entry.split(',').collect();
        if p.len() != 4 || p[3] != "255" {
            return Err(Code::Denied.into());
        }
        peers.push(Peer {
            selector: p[0].parse().map_err(|_| Code::InvalidInput)?,
            public: layerfs_bridge::adapters::native::pipe::key(p[1])?,
            expires_unix: p[2].parse().map_err(|_| Code::InvalidInput)?,
        })
    }
    let sandbox = u128::from_str_radix(&env("LAYERFS_SANDBOX_ID")?, 16)
        .map_err(|_| Code::InvalidInput)?
        .to_be_bytes();
    let instance = layerfs_sandbox::random::<32>()?;
    let state = Arc::new(Mutex::new(State {
        hello: SandboxHelloWire { sandbox, instance },
        live: None,
    }));
    let listener = TcpListener::bind(env("LAYERFS_CONTROL_LISTEN")?)?;
    let mut workers: Vec<thread::JoinHandle<()>> = Vec::new();
    for stream in listener.incoming() {
        let stream = stream?;
        let mut retained = Vec::new();
        for worker in workers.drain(..) {
            if worker.is_finished() {
                worker.join().map_err(|_| Code::Io)?;
            } else {
                retained.push(worker)
            }
        }
        workers = retained;
        if workers.len() >= 4 {
            drop(stream);
            continue;
        }
        let peers = peers.clone();
        let state = state.clone();
        let remote = remote.clone();
        workers.push(thread::spawn(move||{let result=(||{let c=connection::accept(stream,&private,&peers)?;server::serve(c,|_,request,input,output,deadline|{if input.read(&mut [0;1])?!=0{return Err(Code::InvalidInput.into())}let mut state=state.lock().map_err(|_|Code::Io)?;match &request.operation{
Operation::SandboxHello=>Ok(Response::SandboxHello(state.hello.clone())),
Operation::WorkspaceOpen{workspace,incarnation,instance,project,branch,commit}=>{if *instance!=state.hello.instance||state.live.is_some()||commit.is_some(){return Err(Code::Busy.into())}let(s3,base,start)=remote.bootstrap().map_err(bad)?;if *project!=base.stack||*branch!=base.branch||base.head.is_some(){return Err(Code::Unsupported.into())}let name=std::str::from_utf8(workspace).map_err(|_|Code::InvalidInput)?;let path=PathBuf::from("/layerfs/workspace").join(name);std::fs::create_dir_all(&path)?;let backing=PathBuf::from("/layerfs/backing").join(name);let engine=Arc::new(Mutex::new(Engine::create(&backing,start as i64).map_err(bad)?));let mut config=fuser::Config::default();config.acl=fuser::SessionACL::Owner;config.n_threads=Some(1);config.clone_fd=false;config.mount_options=vec![fuser::MountOption::RW,fuser::MountOption::NoSuid,fuser::MountOption::NoDev,fuser::MountOption::DefaultPermissions,fuser::MountOption::Exec,fuser::MountOption::NoAtime,fuser::MountOption::FSName("phase6-sqlite".into()),fuser::MountOption::CUSTOM("max_read=131072".into())];let session=fuser::Session::new(SqlFs{engine:engine.clone()},&path,&config)?.spawn()?;let reader=Reader{s3,locators:Arc::new(remote.clone())};state.live=Some(Live{name:workspace.clone(),incarnation:*incarnation,path,engine,session:Some(session),reader,remote:remote.clone(),base,generation:1,pending:false});Ok(Response::WorkspaceAttach(Box::new(WorkspaceAttachWire{workspace:workspace.clone(),incarnation:*incarnation,outcome:WorkspaceAttachOutcome::Completed})))},
Operation::WorkspaceExec{workspace,incarnation,command}=>{let live=state.live.as_mut().ok_or(Code::NotFound)?;if live.name!=*workspace||live.incarnation!=*incarnation||live.pending{return Err(Code::Denied.into())}crate::execution::execute(&live.path,workspace,*incarnation,command,output,deadline)},
Operation::WorkspaceCommit{workspace,incarnation}=>{let live=state.live.as_mut().ok_or(Code::NotFound)?;if live.name!=*workspace||live.incarnation!=*incarnation{return Err(Code::Denied.into())}if live.pending{return Err(Code::Busy.into())}let mut engine=live.engine.lock().map_err(|_|Code::Io)?;let revision=engine.revision as u64;let mut consumer=Consumer::new(live.reader.clone()).map_err(bad)?;let scope=InodeScope::from_object(layerfs_content::ObjectId::from_bytes(&live.base.scope).map_err(|e|bad(e.to_string()))?);let(candidate,files)=crate::construction::build(&engine,scope,&live.reader,&mut consumer).map_err(bad)?;let root=*candidate.root.0.as_bytes();live.pending=true;let report=live.remote.publish(workspace,*incarnation,live.generation,revision,&live.base,root).map_err(|e|{let mut f=bad(e);f.unknown=true;f})?;let(head,known_root)=match &report.outcome{CommitOutcomeWire::Committed(c)=>(Some(c.commit),c.root),CommitOutcomeWire::UpToDate{head,root}=>(*head,*root)};if known_root!=root{return Err(Code::Integrity.into())}let tx=engine.db.transaction().map_err(|e|bad(e.to_string()))?;for(id,file)in files{tx.execute("UPDATE inodes SET root=?2 WHERE id=?1",rusqlite::params![id,file.as_bytes().as_slice()]).map_err(|e|bad(e.to_string()))?;}tx.execute("UPDATE inodes SET dirty=0",[]).map_err(|e|bad(e.to_string()))?;tx.commit().map_err(|e|bad(e.to_string()))?;live.base.head=head;live.base.root=root;live.generation+=1;live.pending=false;eprintln!("P6 commit generation={} revision={} callbacks={} objects={} pack_bytes={} duplicates={} root={}",report.generation,revision,engine.callbacks,consumer.objects,consumer.bytes,consumer.duplicates,crate::minio::hex(&root));Ok(Response::WorkspaceCommit(Box::new(WorkspaceCommitWire{workspace:workspace.clone(),incarnation:*incarnation,outcome:WorkspaceCommitOutcome::Completed(report)})))},
Operation::WorkspaceUnmount{workspace,incarnation}=>{let live=state.live.as_mut().ok_or(Code::NotFound)?;if live.name!=*workspace||live.incarnation!=*incarnation{return Err(Code::Denied.into())}if live.pending{return Err(Code::Busy.into())}let session=live.session.take().ok_or(Code::NotFound)?;session.umount_and_join()?;let e=live.engine.lock().map_err(|_|Code::Io)?;let handles:i64=e.db.query_row("SELECT count(*) FROM handles",[],|r|r.get(0)).map_err(|e|bad(e.to_string()))?;if handles!=0{return Err(Code::Busy.into())}eprintln!("P6 explicit unmount callbacks={} handles={handles}",e.callbacks);Ok(Response::WorkspaceUnmount(Box::new(WorkspaceLifecycleWire{workspace:workspace.clone(),incarnation:*incarnation,outcome:WorkspaceLifecycleOutcome::Completed})))},_=>Err(Code::Unsupported.into())}})})();if let Err(e)=result{eprintln!("control session ended: {e}")}}));
    }
    Ok(())
}
