//! 自管 Cookie Jar：替代 `cookie_store(true)` 的黑盒 jar。
//!
//! 动因：reqwest 内建 jar 无法查看/清理，登录态问题只能靠"重启应用"解决。
//! 自管 jar 保持相同的自动回放语义（响应 `Set-Cookie` 收纳、同域请求附带），
//! 并提供按域查看/清理（`cookie_list` / `cookie_clear` 命令）。
//!
//! 语义说明（与浏览器/内建 jar 的差异，有意简化）：
//! - 域匹配：cookie 的 Domain 属性（缺省为响应 host）相等或为其父域后缀；
//! - 路径：不做 Path 限制（API 调试场景下限制路径只会造成困惑）；
//! - 过期：`Max-Age<=0`/已过 `Expires` 的在读取时丢弃；会话 cookie 常驻内存。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime};

use reqwest::header::HeaderValue;
use url::Url;

/// 一条收纳的 Cookie。
#[derive(Debug, Clone)]
pub struct StoredCookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub expires_at: Option<SystemTime>,
    pub secure: bool,
    pub http_only: bool,
}

#[derive(Debug, Default)]
struct JarInner {
    /// domain → cookies（同名覆盖）。
    cookies: HashMap<String, Vec<StoredCookie>>,
}

#[derive(Debug, Default)]
pub struct ManagedJar {
    inner: Mutex<JarInner>,
}

impl ManagedJar {
    /// 从一条 `Set-Cookie` 收纳（解析失败静默丢弃，不阻断请求）。
    pub fn store(&self, set_cookie: &str, url: &Url) {
        let Some(cookie) = parse_set_cookie(set_cookie, url) else {
            return;
        };
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        let slot = inner.cookies.entry(cookie.domain.clone()).or_default();
        if let Some(pos) = slot
            .iter()
            .position(|c| c.name == cookie.name && c.path == cookie.path)
        {
            slot[pos] = cookie;
        } else {
            slot.push(cookie);
        }
    }

    /// 取出匹配 url 的全部有效 Cookie（`name=value; ...`）。
    fn header_for(&self, url: &Url) -> Option<HeaderValue> {
        let host = url.host_str()?.to_lowercase();
        let Ok(mut inner) = self.inner.lock() else {
            return None;
        };
        let now = SystemTime::now();
        let mut pairs: Vec<String> = Vec::new();
        for (domain, list) in inner.cookies.iter_mut() {
            if !domain_matches(&host, domain) {
                continue;
            }
            list.retain(|c| c.expires_at.map_or(true, |t| t > now));
            for c in list.iter() {
                if !c.value.is_empty() {
                    pairs.push(format!("{}={}", c.name, c.value));
                }
            }
        }
        if pairs.is_empty() {
            return None;
        }
        HeaderValue::from_str(&pairs.join("; ")).ok()
    }

    /// 按域查看（`domain_filter` 为空返回全部；结果按域名排序）。
    pub fn list(&self, domain_filter: Option<&str>) -> Vec<StoredCookie> {
        let Ok(mut inner) = self.inner.lock() else {
            return Vec::new();
        };
        let now = SystemTime::now();
        let mut out = Vec::new();
        for (domain, list) in inner.cookies.iter_mut() {
            if let Some(f) = domain_filter {
                if !domain.contains(f) {
                    continue;
                }
            }
            list.retain(|c| c.expires_at.map_or(true, |t| t > now));
            out.extend(list.iter().cloned());
        }
        out.sort_by(|a, b| a.domain.cmp(&b.domain).then(a.name.cmp(&b.name)));
        out
    }

    /// 清理（`domain` 为空=全部；否则精确域 + 子域）。返回删除条数。
    pub fn clear(&self, domain: Option<&str>) -> usize {
        let Ok(mut inner) = self.inner.lock() else {
            return 0;
        };
        match domain {
            None => {
                let n: usize = inner.cookies.values().map(Vec::len).sum();
                inner.cookies.clear();
                n
            }
            Some(d) => {
                let d = d.to_lowercase();
                let mut removed = 0;
                inner.cookies.retain(|domain, list| {
                    if domain == &d || domain.ends_with(&format!(".{d}")) {
                        removed += list.len();
                        false
                    } else {
                        true
                    }
                });
                removed
            }
        }
    }

    pub fn len(&self) -> usize {
        self.inner
            .lock()
            .map(|m| m.cookies.values().map(Vec::len).sum())
            .unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

fn domain_matches(host: &str, cookie_domain: &str) -> bool {
    let cd = cookie_domain.strip_prefix('.').unwrap_or(cookie_domain);
    host == cd || host.ends_with(&format!(".{cd}"))
}

fn parse_set_cookie(raw: &str, url: &Url) -> Option<StoredCookie> {
    let mut parts = raw.split(';');
    let first = parts.next()?.trim();
    let (name, value) = first.split_once('=')?;
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let default_domain = url.host_str().unwrap_or("").to_lowercase();
    let mut cookie = StoredCookie {
        name: name.to_string(),
        value: value.trim().to_string(),
        domain: default_domain,
        path: "/".to_string(),
        expires_at: None,
        secure: false,
        http_only: false,
    };
    for attr in parts {
        let attr = attr.trim();
        let (k, v) = match attr.split_once('=') {
            Some((k, v)) => (k.trim().to_lowercase(), v.trim()),
            None => (attr.to_lowercase(), ""),
        };
        match k.as_str() {
            "domain" => {
                if !v.is_empty() {
                    cookie.domain = v.strip_prefix('.').unwrap_or(v).to_lowercase();
                }
            }
            "path" => {
                if !v.is_empty() {
                    cookie.path = v.to_string();
                }
            }
            "max-age" => {
                if let Ok(secs) = v.parse::<i64>() {
                    if secs <= 0 {
                        return None;
                    }
                    cookie.expires_at =
                        SystemTime::now().checked_add(Duration::from_secs(secs as u64));
                }
            }
            "expires" => {
                // 常见 HTTP 日期宽松解析；失败则视为会话 cookie。
                if let Ok(t) = chrono::DateTime::parse_from_rfc2822(v) {
                    cookie.expires_at = Some(t.into());
                } else if let Ok(t) =
                    chrono::NaiveDateTime::parse_from_str(v, "%a, %d %b %Y %H:%M:%S GMT")
                {
                    cookie.expires_at = Some(t.and_utc().into());
                }
            }
            "secure" => cookie.secure = true,
            "httponly" => cookie.http_only = true,
            _ => {}
        }
    }
    Some(cookie)
}

impl reqwest::cookie::CookieStore for ManagedJar {
    fn set_cookies(&self, cookie_headers: &mut dyn Iterator<Item = &HeaderValue>, url: &Url) {
        for value in cookie_headers {
            if let Ok(text) = value.to_str() {
                self.store(text, url);
            }
        }
    }

    fn cookies(&self, url: &Url) -> Option<HeaderValue> {
        self.header_for(url)
    }
}

/// 全局共享 jar（代理切换不再丢弃登录态；原来随旧客户端一起丢弃）。
pub fn shared_jar() -> &'static ManagedJar {
    static JAR: OnceLock<ManagedJar> = OnceLock::new();
    JAR.get_or_init(ManagedJar::default)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn store_and_replay_same_domain() {
        let jar = ManagedJar::default();
        jar.store(
            "sid=abc; Path=/; HttpOnly",
            &url("https://api.example.com/login"),
        );
        let header = jar
            .header_for(&url("https://api.example.com/users"))
            .unwrap();
        assert_eq!(header.to_str().unwrap(), "sid=abc");
        // 不匹配的域不回放。
        assert!(jar.header_for(&url("https://other.com/")).is_none());
    }

    #[test]
    fn subdomain_and_explicit_domain_match() {
        let jar = ManagedJar::default();
        jar.store(
            "t=1; Domain=.example.com; Path=/",
            &url("https://a.example.com/"),
        );
        assert!(jar.header_for(&url("https://b.example.com/x")).is_some());
        assert!(jar.header_for(&url("https://example.com/x")).is_some());
        assert!(jar.header_for(&url("https://notexample.com/")).is_none());
    }

    #[test]
    fn expired_max_age_discarded() {
        let jar = ManagedJar::default();
        jar.store("old=gone; Max-Age=0", &url("https://api.example.com/"));
        assert!(jar.header_for(&url("https://api.example.com/")).is_none());
        assert_eq!(jar.len(), 0);
    }

    #[test]
    fn clear_by_domain_and_all() {
        let jar = ManagedJar::default();
        jar.store("a=1", &url("https://a.com/"));
        jar.store("b=2", &url("https://b.com/"));
        assert_eq!(jar.clear(Some("a.com")), 1);
        assert_eq!(jar.len(), 1);
        assert_eq!(jar.clear(None), 1);
        assert_eq!(jar.len(), 0);
    }
}
