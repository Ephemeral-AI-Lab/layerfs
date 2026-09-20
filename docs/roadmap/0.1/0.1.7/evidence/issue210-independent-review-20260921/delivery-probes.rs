use layerfs_bridge::{contract::*,adapters::native::{client::Client,connection::{accept,connect,Peer,VerifiedPeer},listen,protocol::*}};

#[test]
fn review_legacy_mutation_accepts_result_data_before_failure() {
    let listener=listen("127.0.0.1:0".parse().unwrap()).unwrap();let addr=listener.local_addr().unwrap();
    let ck=[1;32];let sk=[2;32];let sp=*VerifiedPeer::from_private(&sk).unwrap().public_key();
    let peers=[Peer{selector:1,public:*VerifiedPeer::from_private(&ck).unwrap().public_key(),expires_unix:u64::MAX}];
    std::thread::scope(|threads| {
        let server=threads.spawn(|| {
            let (socket,_)=listener.accept().unwrap();let mut c=accept(socket,&sk,&peers).unwrap();
            let hello=c.receive.read().unwrap();c.send.write(&hello).unwrap();
            assert_eq!(c.receive.read().unwrap().kind,Kind::Begin);
            assert_eq!(c.receive.read().unwrap().kind,Kind::EndInput);
            c.send.write(&Frame{kind:Kind::ResultData,id:1,bytes:b"unexpected".to_vec()}).unwrap();
            c.send.write(&Frame{kind:Kind::Failure,id:1,bytes:encode_failure(Code::InvalidInput.into()).to_vec()}).unwrap();
        });
        let mut client=Client::new(connect(addr,1,&ck,&sp).unwrap()).unwrap();let mut input:&[u8]=&[];let mut output=Vec::new();
        let request=Request{id:1,generation:1,store:1,profile:1,deadline_ms:10000,response_bytes:1024,operation:Operation::ConstructFile{length:0}};
        let error=client.call(&request,&mut input,&mut output).unwrap_err();
        println!("legacy mutation output={:?}, failure={error:?}",String::from_utf8_lossy(&output));
        assert_eq!(output,b"unexpected");assert_eq!(error.code,Code::InvalidInput);assert!(!error.unknown);
        server.join().unwrap();
    });
}

#[test]
fn review_codec_accepts_page_above_sixteen_kib() {
    let mut stack=[1;17];stack[0]=0x31;let mut layer=[1;33];layer[0]=0x32;
    let record=StackWire{stack,name:vec![b'a';63],scope:[2;32],profile:[3;32],head_layer:layer};
    let response=Response::History(Box::new(HistoryResult::Stacks{continuation:vec![],records:vec![record;128]}));
    let encoded=encode_response(&response).unwrap();assert!(encoded.len()>16384);
    assert_eq!(decode_response(&encoded).unwrap(),response);println!("codec accepts history page bytes={}",encoded.len());
}
