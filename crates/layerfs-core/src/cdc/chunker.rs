use std::io::Read;

use crate::{CoreError, CoreResult};

use super::gear::GEAR;

include!("chunker/algorithm.rs");
include!("chunker/tests.rs");
