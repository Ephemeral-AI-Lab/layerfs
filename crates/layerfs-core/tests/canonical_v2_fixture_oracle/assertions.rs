#[test]
fn independent_actual_fixture_oracle_freezes_complete_v2_corpus() {
    let expected = [
        (
            1024 * 1024,
            "S1-1",
            "f79de600cf44b20c4443e06d2e2b9e8819e956ba5a7bcc9cab4ffd8a08059cf8",
            "6a1d02f70694a50859c88c0080f0e2cc046c8b0d9e21f474c58dab66a895f1c1",
            53,
            1,
            0,
            0,
            2_025,
            "c2b4a92188569d206717210b596dde9b8aeade1c9c81b87f02b8d0d6ebda1112",
            "b0266bbda936c1532c04fc0155f1efef2fb63d69afb5647952e8f4a10060ab20",
            "2274f609bfbd578a600da5e07b1deed6ff2c9a77927eaba854b0ebf7ab542142",
            "18f33e3ca6030e966cf8ed41c0b43f4769de8b02247f453fae447627bee4b77c",
            "60d191810b303b26d12453add0b9e1718b1f1b654473615d9323f0ee477a9b7d",
            "7e806f7023c3e33914c59d2b0d0d84bca8859fdbd7663b55f5f5c99313252d42",
            "f79de600cf44b20c4443e06d2e2b9e8819e956ba5a7bcc9cab4ffd8a08059cf8",
        ),
        (
            10 * 1024 * 1024,
            "S1-10",
            "e40db05d7407b92253e56099df402f03b399990014b2d1397e422ca305472449",
            "982e992203cd527c1b7147e4e9509bcd2e5828706fc2313f18bcfe1b4de2f3ed",
            531,
            9,
            0,
            0,
            19_777,
            "8eb047a5d7ac6cc86c26d30d014c46f722936147a0989683303057c96fbec67c",
            "f119169a3aee39fdac17b72197dd5429155a34524ec7b02af421037e8deace08",
            "8ad4351bb76bac1b0a80e279d8a5225a5ff752bce73c569f70daa7a15b79a0bf",
            "003fac659363e97667cc75fa8fb81fef7065b856c547440e22722b76c1e72342",
            "001cdef1e85c266038e98bc86e8470dc1b9d21e021bac1abd0d03e994e42c440",
            "35282fcfecc493c025a3bc4a7567efc12562fc8a4d863c88e07617fb5e97d1c9",
            "08a1490b77d2afcc6fc9149e24bfe66696735791d7a7c370d2e1a31076ecaa1d",
        ),
        (
            100 * 1024 * 1024,
            "S1-100",
            "bb883eecf4ea85d80432953791dcc352243da94175e7503e2c476afe9bd0bab7",
            "5bb376c3c54d8724973a7b160acab599f2f5cee4b4a56e855ff0cbe987425994",
            5_284,
            83,
            2,
            1,
            196_055,
            "5bf63ef8adad7bb373be4e968759997b385590d076d75a1488e1958b64a3f8e2",
            "c7107d5f0ecd8bd8a9efe11bde900aa50dbbff49dfc3122000835dc1323e1ecd",
            "6f923dfa4f32981884af0437476f9c4e8b7f4bb1af84ecc6420a48daa455713c",
            "93d1b461b5cbf88e8d122ad4e90a15a6e3029c408f0fe02ef51c561e5a94c6d1",
            "2de8d2ce6b614373ba8fb8b29d3a3eccd535abfccc2654e7922514e5ca90fd89",
            "29233d6018b031f6035c8e2c8175f1ab86fb721808f21a8858bc965567e9c0c1",
            "0a4b6b60703a8b25d01b990ec346f5ab26661367c56de210b21769947692cd0f",
        ),
    ];
    for (
        size,
        label,
        fingerprint,
        raw_sequence,
        refs,
        leaves,
        branches,
        level,
        mapping_bytes,
        commitment,
        corpus,
        file_root,
        workspace_root,
        transition,
        closure,
        range,
    ) in expected
    {
        let actual = oracle(size, label);
        println!(
            "{label}\tcommitment={}\tcorpus={}\tfile_root={}\tworkspace_root={}\ttransition={}\tclosure={}\trange={}",
            actual.commitment,
            actual.corpus,
            actual.file_root,
            actual.workspace_root,
            actual.transition,
            actual.closure,
            actual.range,
        );
        assert_eq!(actual.label, label);
        assert_eq!(actual.source_fingerprint, fingerprint);
        assert_eq!(actual.raw_sequence, raw_sequence);
        assert_eq!(actual.references, refs);
        assert_eq!(actual.leaves, leaves);
        assert_eq!(actual.branches, branches);
        assert_eq!(actual.level, level);
        assert_eq!(actual.mapping_bytes, mapping_bytes);
        assert_eq!(
            actual.mapping_bytes + 119,
            match label {
                "S1-1" => 2_144,
                "S1-10" => 19_896,
                "S1-100" => 196_174,
                _ => unreachable!(),
            }
        );
        assert_eq!(actual.reconstruction, fingerprint);
        assert_eq!(actual.commitment, commitment);
        assert_eq!(actual.corpus, corpus);
        assert_eq!(actual.file_root.to_string(), file_root);
        assert_eq!(actual.workspace_root.to_string(), workspace_root);
        assert_eq!(actual.transition.to_string(), transition);
        assert_eq!(actual.closure, closure);
        assert_eq!(actual.range, range);
    }
}
