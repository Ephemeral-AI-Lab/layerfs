//! `docker` network: the host port `docker port` reports.
/// The host port of `docker port`'s answer (`127.0.0.1:55012`, maybe a second IPv6 line).
pub(crate) fn published_port(answer: &str) -> Option<u16> {
    answer
        .lines()
        .find_map(|line| line.rsplit_once(':')?.1.trim().parse().ok())
}
