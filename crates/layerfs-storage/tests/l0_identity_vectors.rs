const STRUCTURAL_VECTORS: [(&str, &str, &str); 9] = [
    (
        "logical_chunk_abc",
        "455356322d4c4348554e4b0001000300000000000000616263",
        "1174c050f4ebe0866002fcd0a52001f0418159dc0c1d2d98e85c14e16a13c164",
    ),
    (
        "logical_file_abc",
        "455356322d4c46494c450001000300000000000000010000001174c050f4ebe0866002fcd0a52001f0418159dc0c1d2d98e85c14e16a13c1640300000000000000",
        "c54ded3a17e29e554f21791a488787aadca8241b23e727fd5459ae42e7013d32",
    ),
    (
        "file_node_0644_abc",
        "455356322d464e4f4445000100a401c54ded3a17e29e554f21791a488787aadca8241b23e727fd5459ae42e7013d320300000000000000",
        "82204b82869a1532b0c2bddfadcfcc3fd15c2ae78dbde925367d9d49d25e56ee",
    ),
    (
        "symlink_node_file_txt",
        "455356322d534e4f44450001000800000066696c652e747874",
        "b09cb3ee0185d96abb9200d1731e74e65a3a97ffe113b14350ad88114ec15236",
    ),
    (
        "directory_explicit_0755_nested_file",
        "455356322d444e4f4445000100ed010100000004000000646174610182204b82869a1532b0c2bddfadcfcc3fd15c2ae78dbde925367d9d49d25e56ee",
        "00768e2a70807c641a519cee8544ad1038cd6c769ae4cab05e2c24bfb5b3f466",
    ),
    (
        "directory_implicit_empty_root_1000",
        "455356322d444e4f4445000100001000000000",
        "b72af46dec9bbef3e7818f7facf4541b21b3b3ef6e5b30be51301c16fed6dd09",
    ),
    (
        "version_empty_root",
        "455356322d56524f4f54000100b72af46dec9bbef3e7818f7facf4541b21b3b3ef6e5b30be51301c16fed6dd09",
        "44b0eb7c80a93ffc3cb98e4ff16c90d4a8549b0c7c0e86e0d3ee2a857b300963",
    ),
    (
        "directory_implicit_composite_root_1000",
        "455356322d444e4f44450001000010030000000800000066696c652e7478740182204b82869a1532b0c2bddfadcfcc3fd15c2ae78dbde925367d9d49d25e56ee040000006c696e6b03b09cb3ee0185d96abb9200d1731e74e65a3a97ffe113b14350ad88114ec15236060000006e65737465640200768e2a70807c641a519cee8544ad1038cd6c769ae4cab05e2c24bfb5b3f466",
        "70ef59cbf243c0a9c44e26001c6d3deaa01946ea2cd06a2e4b0c87ade1cadd26",
    ),
    (
        "version_composite",
        "455356322d56524f4f5400010070ef59cbf243c0a9c44e26001c6d3deaa01946ea2cd06a2e4b0c87ade1cadd26",
        "f2dfceb5f1618031b99634897ddc5c760421fcd92b53f8715b776a127e40effa",
    ),
];

fn decode_hex(input: &str) -> Vec<u8> {
    assert_eq!(input.len() % 2, 0, "hex has an odd number of nibbles");
    input
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = (pair[0] as char).to_digit(16).expect("high nibble");
            let low = (pair[1] as char).to_digit(16).expect("low nibble");
            ((high << 4) | low) as u8
        })
        .collect()
}

#[test]
fn frozen_m60_structural_preimages_recompute_exactly() {
    for (name, preimage_hex, expected_id) in STRUCTURAL_VECTORS {
        let preimage = decode_hex(preimage_hex);
        let expected_id = decode_hex(expected_id);
        assert_eq!(expected_id.len(), 32, "{name} digest width");
        assert_eq!(preimage.len() * 2, preimage_hex.len(), "{name} byte count");
        assert_eq!(
            blake3::hash(&preimage).as_bytes(),
            expected_id.as_slice(),
            "{name}"
        );
    }
}

#[test]
fn frozen_fixture_text_contains_the_authoritative_vector_and_receipt_markers() {
    let golden = include_str!("../../../tests/fixtures/frozen/m6-0/M6_0_GOLDEN_VECTORS.md");
    let change_receipt =
        include_str!("../../../tests/fixtures/frozen/m6-0/M6_0_CHANGE_RECEIPT_V1.md");
    let generator_readme = include_str!("../../../tests/fixtures/frozen/m6-0/m6-vectors/README.md");
    let receipt =
        include_str!("../../../tests/fixtures/frozen/m6-1-2/M6_1_2_VERIFICATION_RECEIPT.md");
    let spec = include_str!("../../../tests/fixtures/frozen/m6-1-2/M6_1_SPEC.md");

    for name in [
        "logical_chunk_abc",
        "logical_file_abc",
        "file_node_0644_abc",
        "symlink_node_file_txt",
        "directory_explicit_0755_nested_file",
        "directory_implicit_empty_root_1000",
        "version_empty_root",
        "directory_implicit_composite_root_1000",
        "version_composite",
    ] {
        assert!(golden.contains(name), "missing frozen vector {name}");
    }
    assert!(change_receipt.contains("CONTRACT_STATUS: FROZEN_M6_0_CONTRACT_COMPLETE"));
    assert!(generator_readme.contains("DOCUMENTATION-ONLY"));
    assert!(receipt.contains("RECEIPT_STATE: FINAL_PASS"));
    assert!(receipt.contains("M6_1_2_CRITERIA_PASS: 6_OF_6"));
    assert!(spec.contains("M6.1"));
}
