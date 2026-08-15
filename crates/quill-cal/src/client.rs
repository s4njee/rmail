//! CalDAV HTTP / WebDAV client implementation (Roadmap 1.4 / RFC 4791 / RFC 6578).

use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE, IF_MATCH, IF_NONE_MATCH};
use reqwest::{Client, Method, StatusCode};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct CalDavClient {
    client: Client,
    base_url: String,
    auth_header: HeaderValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalDavCollection {
    pub href: String,
    pub display_name: String,
    pub color: Option<String>,
    pub ctag: Option<String>,
    pub sync_token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalDavResource {
    pub href: String,
    pub etag: String,
    pub ical_data: String,
}

impl CalDavClient {
    pub fn new(base_url: &str, username: &str, password: &str) -> Result<Self, String> {
        let auth_str = format!("{username}:{password}");
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(auth_str.as_bytes());
        let auth_header = HeaderValue::from_str(&format!("Basic {b64}"))
            .map_err(|e| format!("invalid auth header: {e}"))?;

        let client = Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|e| format!("failed to build HTTP client: {e}"))?;

        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            auth_header,
        })
    }

    fn headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(reqwest::header::AUTHORIZATION, self.auth_header.clone());
        headers.insert(reqwest::header::USER_AGENT, HeaderValue::from_static("Quill-CalDAV/1.0"));
        headers
    }

    /// Discover principal and calendar-home-set URL from server.
    pub async fn discover_calendar_home(&self) -> Result<String, String> {
        // Step 1: Probe well-known or base URL with PROPFIND Depth: 0
        let xml = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propfind xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:prop>
    <D:current-user-principal />
    <C:calendar-home-set />
  </D:prop>
</D:propfind>"#;

        let mut url = self.base_url.clone();
        let mut resp = self.send_webdav("PROPFIND", &url, Some("0"), Some(xml), "application/xml").await?;

        if !resp.status().is_success() {
            // Try with /.well-known/caldav
            url = format!("{}/.well-known/caldav", self.base_url);
            resp = self.send_webdav("PROPFIND", &url, Some("0"), Some(xml), "application/xml").await?;
        }

        let body = resp.text().await.map_err(|e| format!("failed to read propfind response: {e}"))?;

        // Extract calendar-home-set or principal href
        if let Some(home) = extract_xml_tag_content(&body, "calendar-home-set") {
            if let Some(href) = extract_xml_tag_content(&home, "href") {
                return Ok(self.resolve_url(&href));
            }
        }

        if let Some(principal) = extract_xml_tag_content(&body, "current-user-principal") {
            if let Some(href) = extract_xml_tag_content(&principal, "href") {
                let principal_url = self.resolve_url(&href);
                // Query principal URL for calendar-home-set
                let p_resp = self.send_webdav("PROPFIND", &principal_url, Some("0"), Some(xml), "application/xml").await?;
                let p_body = p_resp.text().await.map_err(|e| e.to_string())?;
                if let Some(home) = extract_xml_tag_content(&p_body, "calendar-home-set") {
                    if let Some(href) = extract_xml_tag_content(&home, "href") {
                        return Ok(self.resolve_url(&href));
                    }
                }
            }
        }

        // Fallback to base URL if discovery tags are omitted (e.g. Radicale/Nextcloud direct path)
        Ok(self.base_url.clone())
    }

    /// List calendar collections in the calendar home.
    pub async fn list_calendars(&self, home_url: &str) -> Result<Vec<CalDavCollection>, String> {
        let xml = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propfind xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav" xmlns:CS="http://calendarserver.org/ns/" xmlns:IC="http://apple.com/ns/ical/">
  <D:prop>
    <D:resourcetype />
    <D:displayname />
    <CS:getctag />
    <D:sync-token />
    <IC:calendar-color />
  </D:prop>
</D:propfind>"#;

        let resp = self.send_webdav("PROPFIND", home_url, Some("1"), Some(xml), "application/xml").await?;
        let body = resp.text().await.map_err(|e| e.to_string())?;
        Ok(parse_multistatus_calendars(&body, home_url))
    }

    /// Fetch all events within a calendar collection.
    pub async fn fetch_events(&self, calendar_url: &str) -> Result<Vec<CalDavResource>, String> {
        let xml = r#"<?xml version="1.0" encoding="utf-8" ?>
<C:calendar-query xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:prop>
    <D:getetag />
    <C:calendar-data />
  </D:prop>
  <C:filter>
    <C:comp-filter name="VCALENDAR">
      <C:comp-filter name="VEVENT" />
    </C:comp-filter>
  </C:filter>
</C:calendar-query>"#;

        let resp = self.send_webdav("REPORT", calendar_url, Some("1"), Some(xml), "application/xml").await?;
        let body = resp.text().await.map_err(|e| e.to_string())?;
        Ok(parse_multistatus_events(&body))
    }

    /// Upload or update an event iCalendar file (PUT).
    pub async fn put_event(
        &self,
        event_url: &str,
        ical_data: &str,
        etag: Option<&str>,
    ) -> Result<String, String> {
        let mut req = self
            .client
            .put(event_url)
            .headers(self.headers())
            .header(CONTENT_TYPE, "text/calendar; charset=utf-8")
            .body(ical_data.to_string());

        if let Some(etag_val) = etag {
            req = req.header(IF_MATCH, etag_val);
        } else {
            req = req.header(IF_NONE_MATCH, "*");
        }

        let resp = req.send().await.map_err(|e| format!("PUT failed: {e}"))?;
        let status = resp.status();

        if status == StatusCode::PRECONDITION_FAILED {
            return Err("conflict: 412 Precondition Failed".into());
        }

        if !status.is_success() && status != StatusCode::NO_CONTENT && status != StatusCode::CREATED {
            return Err(format!("PUT returned status {status}"));
        }

        let new_etag = resp
            .headers()
            .get("ETag")
            .and_then(|h| h.to_str().ok())
            .unwrap_or_default()
            .to_string();

        Ok(new_etag)
    }

    /// Delete an event from server (DELETE).
    pub async fn delete_event(&self, event_url: &str, etag: Option<&str>) -> Result<(), String> {
        let mut req = self.client.delete(event_url).headers(self.headers());
        if let Some(etag_val) = etag {
            req = req.header(IF_MATCH, etag_val);
        }
        let resp = req.send().await.map_err(|e| format!("DELETE failed: {e}"))?;
        let status = resp.status();
        if status == StatusCode::PRECONDITION_FAILED {
            return Err("conflict: 412 Precondition Failed".into());
        }
        if !status.is_success() && status != StatusCode::NOT_FOUND && status != StatusCode::NO_CONTENT {
            return Err(format!("DELETE returned status {status}"));
        }
        Ok(())
    }

    async fn send_webdav(
        &self,
        method_str: &str,
        url: &str,
        depth: Option<&str>,
        body: Option<&str>,
        content_type: &str,
    ) -> Result<reqwest::Response, String> {
        let method = Method::from_bytes(method_str.as_bytes())
            .map_err(|e| format!("invalid method {method_str}: {e}"))?;
        let mut req = self.client.request(method, url).headers(self.headers());

        if let Some(d) = depth {
            req = req.header("Depth", d);
        }
        if let Some(b) = body {
            req = req.header(CONTENT_TYPE, content_type).body(b.to_string());
        }

        req.send().await.map_err(|e| format!("WebDAV request {method_str} failed: {e}"))
    }

    fn resolve_url(&self, path_or_url: &str) -> String {
        resolve_href(&self.base_url, path_or_url)
    }
}

/// Resolve a possibly-relative href against a base URL, preserving the base's
/// path prefix — a CalDAV install commonly lives under `/caldav/user/…`, and
/// rooting relative hrefs at the host would point every request at the wrong
/// path.
fn resolve_href(base: &str, href: &str) -> String {
    if href.starts_with("http://") || href.starts_with("https://") {
        return href.to_string();
    }
    match reqwest::Url::parse(base).and_then(|b| b.join(href)) {
        Ok(joined) => joined.to_string(),
        Err(_) => href.to_string(),
    }
}

/// Decode the XML character references CalDAV servers use to escape
/// `calendar-data` and display names on the wire (`&amp; &lt; &gt; &quot; &apos;`).
fn unescape_xml(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn extract_xml_tag_content(xml: &str, tag_name: &str) -> Option<String> {
    let lower_xml = xml.to_lowercase();
    let lower_tag = tag_name.to_lowercase();

    // Look for <tag_name or <prefix:tag_name
    let mut pos = 0;
    while let Some(open_idx) = lower_xml[pos..].find('<') {
        let actual_open = pos + open_idx;
        let rest = &lower_xml[actual_open + 1..];
        let tag_name_end = match rest.find(|c: char| c == '>' || c.is_whitespace() || c == '/') {
            Some(idx) => idx,
            None => break,
        };
        let found_tag = &rest[..tag_name_end];
        let base_found = found_tag.split(':').last().unwrap_or(found_tag);

        if base_found == lower_tag {
            // Find end of opening tag '>'
            let tag_close_idx = match xml[actual_open..].find('>') {
                Some(idx) => actual_open + idx,
                None => break,
            };

            // If self-closing tag like <tag_name />, return empty
            if xml[actual_open..tag_close_idx].ends_with('/') {
                return Some(String::new());
            }

            let content_start = tag_close_idx + 1;

            // Find closing tag </...tag_name>
            let mut search_pos = content_start;
            while let Some(c_idx) = lower_xml[search_pos..].find("</") {
                let actual_c = search_pos + c_idx;
                let c_rest = &lower_xml[actual_c + 2..];
                if let Some(c_end) = c_rest.find('>') {
                    let c_tag = &c_rest[..c_end];
                    let c_base = c_tag.split(':').last().unwrap_or(c_tag);
                    if c_base == lower_tag {
                        return Some(xml[content_start..actual_c].trim().to_string());
                    }
                    search_pos = actual_c + 2 + c_end;
                } else {
                    break;
                }
            }
        }
        pos = actual_open + 1;
    }

    None
}

pub fn parse_multistatus_calendars(xml: &str, base: &str) -> Vec<CalDavCollection> {
    let mut collections = Vec::new();
    let delimiter = if xml.contains("<D:response") {
        "<D:response"
    } else {
        "<response"
    };

    for chunk in xml.split(delimiter).skip(1) {
        let is_calendar = chunk.contains("<C:calendar")
            || chunk.contains("<calendar")
            || chunk.contains("<c:calendar");
        if !is_calendar {
            continue;
        }

        let href = extract_xml_tag_content(chunk, "href").unwrap_or_default();
        if href.is_empty() {
            continue;
        }
        // CalDAV servers commonly return root-relative (or request-relative)
        // hrefs; resolve them against the base before they are used as request
        // URLs, or every sync fails with RelativeUrlWithoutBase.
        let href = resolve_href(base, &href);

        let display_name = unescape_xml(
            &extract_xml_tag_content(chunk, "displayname").unwrap_or_else(|| {
                href.trim_matches('/').split('/').last().unwrap_or("Calendar").to_string()
            }),
        );

        let ctag = extract_xml_tag_content(chunk, "getctag");
        let sync_token = extract_xml_tag_content(chunk, "sync-token");
        let color = extract_xml_tag_content(chunk, "calendar-color");

        collections.push(CalDavCollection {
            href,
            display_name,
            color,
            ctag,
            sync_token,
        });
    }

    collections
}

pub fn parse_multistatus_events(xml: &str) -> Vec<CalDavResource> {
    let mut resources = Vec::new();
    let delimiter = if xml.contains("<D:response") {
        "<D:response"
    } else {
        "<response"
    };

    for chunk in xml.split(delimiter).skip(1) {
        let href = extract_xml_tag_content(chunk, "href").unwrap_or_default();
        let etag = extract_xml_tag_content(chunk, "getetag").unwrap_or_default();
        // calendar-data is XML-escaped on the wire; without decoding, a
        // `SUMMARY:A & B` arrives as the literal text "A &amp; B".
        let ical_data =
            unescape_xml(&extract_xml_tag_content(chunk, "calendar-data").unwrap_or_default());

        if !href.is_empty() && (!etag.is_empty() || !ical_data.is_empty()) {
            resources.push(CalDavResource {
                href,
                etag: etag.trim_matches('"').to_string(),
                ical_data,
            });
        }
    }

    resources
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_multistatus_calendars() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav" xmlns:CS="http://calendarserver.org/ns/">
  <D:response>
    <D:href>/caldav/user/home/</D:href>
    <D:propstat>
      <D:prop>
        <D:resourcetype><D:collection/></D:resourcetype>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
  <D:response>
    <D:href>/caldav/user/home/work/</D:href>
    <D:propstat>
      <D:prop>
        <D:resourcetype><D:collection/><C:calendar/></D:resourcetype>
        <D:displayname>Work Calendar</D:displayname>
        <CS:getctag>ctag-12345</CS:getctag>
        <D:sync-token>sync-token-999</D:sync-token>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;

        let list = parse_multistatus_calendars(xml, "https://cal.example.com");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].display_name, "Work Calendar");
        assert_eq!(list[0].href, "https://cal.example.com/caldav/user/home/work/");
        assert_eq!(list[0].ctag.as_deref(), Some("ctag-12345"));
        assert_eq!(list[0].sync_token.as_deref(), Some("sync-token-999"));
    }

    #[test]
    fn test_parse_multistatus_calendars_relative_and_base_path() {
        // Root-relative and request-relative hrefs both resolve against the
        // base, preserving its path prefix.
        let xml = r#"<multistatus xmlns="DAV:">
<response><href>/caldav/user/home/personal/</href><propstat><prop><resourcetype><collection/><calendar/></resourcetype></prop><status>HTTP/1.1 200 OK</status></propstat></response>
<response><href>work/</href><propstat><prop><resourcetype><collection/><calendar/></resourcetype><displayname>Team &amp; Company</displayname></prop><status>HTTP/1.1 200 OK</status></propstat></response>
</multistatus>"#;
        let list = parse_multistatus_calendars(xml, "https://cal.example.com/caldav/user/home/");
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].href, "https://cal.example.com/caldav/user/home/personal/");
        assert_eq!(list[1].href, "https://cal.example.com/caldav/user/home/work/");
        assert_eq!(list[1].display_name, "Team & Company");
    }

    #[test]
    fn test_parse_multistatus_events() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <D:href>/caldav/user/work/event1.ics</D:href>
    <D:propstat>
      <D:prop>
        <D:getetag>"etag-abc"</D:getetag>
        <C:calendar-data>BEGIN:VCALENDAR
VERSION:2.0
BEGIN:VEVENT
UID:uid-1
SUMMARY:Team Sync
END:VEVENT
END:VCALENDAR</C:calendar-data>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;

        let events = parse_multistatus_events(xml);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].etag, "etag-abc");
        assert!(events[0].ical_data.contains("SUMMARY:Team Sync"));
    }
}
