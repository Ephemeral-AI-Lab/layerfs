macro_rules! lifecycle_callbacks {
    () => {
        fn init(&mut self, _request: &Request, config: &mut KernelConfig) -> std::io::Result<()> {
            let _callback = self.callback();
            self.count(|counters| counters.init += 1);
            config
                .set_max_write(MAX_REQUEST_BYTES as u32)
                .map_err(|maximum| std::io::Error::other(format!("kernel max_write {maximum}")))?;
            let _ = config.set_max_readahead(MAX_REQUEST_BYTES as u32);
            config.set_max_background(8).map_err(|minimum| {
                std::io::Error::other(format!("kernel max_background {minimum}"))
            })?;
            config
                .set_time_granularity(Duration::from_nanos(1))
                .map_err(|granularity| {
                    std::io::Error::other(format!("kernel time granularity {granularity:?}"))
                })?;
            Ok(())
        }

        fn destroy(&mut self) {
            let _callback = self.callback();
            self.count(|counters| counters.destroy += 1);
            self.session_end.notify();
        }
    };
}
