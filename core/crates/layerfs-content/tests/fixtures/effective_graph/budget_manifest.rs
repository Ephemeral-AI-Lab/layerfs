// Independent prospective budget literals; see budget_vectors.py.
pub const BUDGET_CASES: &[(&str,u64,bool,u64,u64,u64)] = &[
("zero",0,false,0,0,0),
("below_minimum",16773120,false,0,0,0),
("unaligned",16777217,false,0,0,0),
("default16",16777216,true,65536,4128768,4096),
("configured32",33554432,true,131072,8257536,8192),
("configured48",50331648,true,196608,12386304,12288),
("configured64",67108864,true,262144,16515072,16384),
("format_maximum_unqualified",1099511623680,true,4294967280,270582938640,268435455),
("above_format_range",1099511627776,false,0,0,0),
];
