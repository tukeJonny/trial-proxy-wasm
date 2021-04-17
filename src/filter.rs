use crate::clock::RateLimitFilterClock;
use log::warn;
use limitador::storage::wasm::WasmStorage;
use proxy_wasm::traits::*;
use proxy_wasm::types::*;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, SystemTime};

// Root context
pub struct RateLimitFilterRoot;

impl Context for RateLimitFilterRoot {}

impl RootContext for RateLimitFilterRoot {
    fn get_type(&self) -> Option<ContextType> {
        Some(ContextType::HttpContext)
    }

    fn create_http_context(&self, _: u32) -> Option<Box<dyn HttpContext>> {
        Some(Box::new(RateLimitFilter))
    }
}

// HTTP context
struct RateLimitFilter;

impl<'a> RateLimitFilter {
    const NAMESPACE: &'a str = "ratelimitfilter";
    const KEY: &'a str = "counters";

    fn get_ratelimit_counters(&self) -> Result<HashMap<limitador::counter::Counter, SystemTime>, String> {
        let (shared_data, _) = self.get_shared_data(RateLimitFilter::KEY);
        match shared_data {
            Some(data) => {
                let deserialized = bincode::deserialize::<HashMap<limitador::counter::Counter, SystemTime>>(&data[..]);
                match deserialized {
                    Ok(counters) => Ok(counters),
                    Err(e) => Err(format!("failed to deserialize ratelimit counters: {:?}", e)),
                }
            }
            None => Ok(HashMap::new()),
        }
    }

    fn set_ratelimit_counters(&self, counters: HashSet<limitador::counter::Counter>) -> Result<(), String> {
        let mut shared_data: HashMap<limitador::counter::Counter, SystemTime> = HashMap::new();
        for counter in counters {
            let now = match proxy_wasm::hostcalls::get_current_time() {
                Ok(current_time) => current_time,
                Err(status) => return Err(format!("failed to get current time with status={:?}", status)),
            };
            let expires_in = counter.expires_in().unwrap_or(Duration::from_nanos(0));

            shared_data.insert(counter.clone(), now + expires_in);
        }

        let serialized = bincode::serialize(&shared_data).expect("failed to serialize ratelimit counters");
        self.set_shared_data(RateLimitFilter::KEY, Some(&serialized), None)
            .expect("failed to set ratelimit counters");

        Ok(())
    }

    fn extract_required_headers(&self, request_headers: &[(String, String)]) -> HashMap<String, String> {
        let mut required_headers: HashMap<String, String> = HashMap::new();
        for (key, value) in request_headers {
            if key.starts_with(':') {
                if *key == ":method" {
                    required_headers.insert("req.method".to_string(), value.to_string());
                }
            } else {
                // for non HTTP/2 header
                required_headers.insert(format!("req.headers.{}", key.to_lowercase()), value.to_string());
            }
        }

        required_headers
    }

    fn make_limiter_per_req(&self, storage: WasmStorage) -> Result<limitador::RateLimiter, String> {
        let limiter_storage: Box<dyn limitador::storage::Storage> = Box::new(storage);
        let limiter = limitador::RateLimiter::new_with_storage(limiter_storage);

        match limiter.add_limit(&limitador::limit::Limit::new(
            "ratelimitfilter",
            3, // カウンタの最大値
            20, // 20秒ごと
            vec!["req.method == GET"],
            vec!["req.headers.x-user-id"],
        )) {
            Ok(_) => (),
            Err(e) => return Err(format!("failed to add ratelimit for GET request: {:?}", e)),
        }

        Ok(limiter)
    }
}

impl Context for RateLimitFilter {}

impl HttpContext for RateLimitFilter {
    fn on_http_request_headers(&mut self, _: usize) -> Action {
        let clock = Box::new(RateLimitFilterClock);
        let storage = WasmStorage::new(clock);

        let limiter = if let Ok(counters) = self.get_ratelimit_counters() {
            counters.iter().for_each(|(counter, expires_at)| {
                let remaining_time = counter.remaining().unwrap_or(0);
                storage.add_counter(counter, remaining_time, *expires_at);
            });

            match self.make_limiter_per_req(storage) {
                Ok(limiter) => limiter,
                Err(e) => {
                    warn!("failed to make ratelimiter: {:?}", e);
                    return Action::Pause;
                }
            }
        } else {
            warn!("failed to get ratelimit counters");
            return Action::Pause;
        };

        let http_headers = self.get_http_request_headers();
        let required_headers = self.extract_required_headers(&http_headers);

        if let Ok(is_limited) = limiter.is_rate_limited(RateLimitFilter::NAMESPACE, &required_headers, 1) {
            if is_limited {
                self.send_http_response(429, vec![], Some(b"Too many requests.\n"));
                return Action::Pause
            }
        } else {
            warn!("failed to check ratelimit status by limitador");
            return Action::Pause
        }

        if let Err(e) = limiter.update_counters(RateLimitFilter::NAMESPACE, &required_headers, 1) {
            warn!("failed to update ratelimit counters by limitador: {:?}", e);
            return Action::Pause;
        }

        match limiter.get_counters("ratelimitfilter") {
            Ok(counters) => {
                if let Err(e) = self.set_ratelimit_counters(counters) {
                    warn!("failed to set ratelimit counters: {:?}", e);
                    return Action::Pause;
                }
            }
            Err(e) => {
                warn!("failed to get ratelimit counters by limitador: {:?}", e);
                return Action::Pause;
            }
        };

        Action::Continue
    }
}