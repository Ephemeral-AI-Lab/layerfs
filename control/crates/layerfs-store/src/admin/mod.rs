//! The Store calls that manage a Store rather than read or change its
//! data: the ones that check its users (`with_users`) and the host
//! worker's Store side (`host_worker`).
pub mod host_worker;
pub mod maintenance;
pub mod open;
pub mod record;
pub mod route;
pub mod status;
pub mod with_users;
