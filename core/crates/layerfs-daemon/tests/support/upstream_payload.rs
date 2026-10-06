//! Independent native fixture/oracle bytes for the external functional proof.
pub const RAW_TARGET: &[u8] = b"../output/raw-\xff-target";
pub const FILES: &[(&str, &[u8])] = &[
    (".git/index", b"DIRC\0\x01\xffindex"),
    (".gitignore", b".ignored.tmp\n.cache/\noutput/\n"),
    (".ignored.tmp", b"ignored bytes remain in the filesystem"),
    (".cache/dependency/cache.bin", b"dependency and cache bytes"),
    ("output/result.bin", b"output bytes"),
    ("shared.bin", b"one regular inode with two names"),
    ("shared-alias.bin", b"one regular inode with two names"),
];
