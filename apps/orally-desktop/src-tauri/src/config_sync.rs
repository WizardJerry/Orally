//! Require two matching reads before accepting an external editor's write.
use orally_config::AppConfig;

#[derive(Default)]
pub(super) struct ConfigSync {
    candidate: Option<AppConfig>,
    applied: Option<AppConfig>,
    last_error: Option<String>,
}

impl ConfigSync {
    pub(super) fn observe(
        &mut self,
        incoming: Result<AppConfig, String>,
    ) -> Result<Option<Box<AppConfig>>, String> {
        let config = match incoming {
            Ok(config) => config,
            Err(error) => {
                self.candidate = None;
                return if self.report_error(&error) {
                    Err(error)
                } else {
                    Ok(None)
                };
            }
        };
        if self.candidate.as_ref() != Some(&config) {
            self.candidate = Some(config);
            return Ok(None);
        }
        if self.applied.as_ref() == Some(&config) && self.last_error.is_none() {
            return Ok(None);
        }
        Ok(Some(Box::new(config)))
    }

    pub(super) fn applied(&mut self, config: AppConfig) {
        self.applied = Some(config);
        self.last_error = None;
    }

    pub(super) fn report_error(&mut self, error: &str) -> bool {
        if self.last_error.as_deref() == Some(error) {
            return false;
        }
        self.last_error = Some(error.to_string());
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ignores_partial_writes_and_recovers_after_errors() {
        let mut sync = ConfigSync::default();
        let mut config = AppConfig::default();
        assert!(sync.observe(Ok(config.clone())).unwrap().is_none());
        assert!(sync.observe(Ok(config.clone())).unwrap().is_some());
        sync.applied(config.clone());
        assert!(sync.observe(Ok(config.clone())).unwrap().is_none());
        assert!(sync.observe(Err("invalid TOML".into())).is_err());
        assert!(sync.observe(Err("invalid TOML".into())).unwrap().is_none());
        config.appearance.dark_mode = false;
        assert!(sync.observe(Ok(config.clone())).unwrap().is_none());
        let restored = sync.observe(Ok(config.clone())).unwrap().unwrap();
        assert!(!restored.appearance.dark_mode);
        sync.applied(*restored);
        assert!(sync.observe(Ok(config)).unwrap().is_none());
    }
}
