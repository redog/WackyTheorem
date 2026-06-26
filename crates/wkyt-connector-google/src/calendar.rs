//! Google Calendar API client.
//!
//! Fetches events from the user's primary calendar using the Calendar v3
//! API. Supports both full sync (last 30+ days) and incremental sync via
//! Google's `syncToken` / `nextPageToken` mechanism.
//!
//! The response `syncToken` maps directly to our [`SyncToken`]: Google
//! hands us an opaque string that we persist in the vault and hand back
//! on the next sync. If Google returns HTTP 410 GONE, the token is stale
//! and we surface [`SyncError::ResyncRequired`].

use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{debug, warn};
use wkyt_core::{Delta, DeltaBatch, Item, ItemKind, SyncError, SyncToken};

fn calendar_api_base() -> String {
    std::env::var("WKYT_MOCK_CALENDAR_API_BASE")
        .unwrap_or_else(|_| "https://www.googleapis.com/calendar/v3".to_string())
}
/// Spec DoD #5: ≥30 days. We fetch 90 for comfort.
const DEFAULT_LOOKBACK_DAYS: i64 = 90;
const MAX_RESULTS_PER_PAGE: u32 = 250;

/// One page of events from the Calendar API.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EventsListResponse {
    items: Option<Vec<CalendarEvent>>,
    next_page_token: Option<String>,
    next_sync_token: Option<String>,
}

/// Minimal Calendar event structure — we extract what Phase 1 needs.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CalendarEvent {
    id: String,
    status: Option<String>,
    summary: Option<String>,
    description: Option<String>,
    location: Option<String>,
    start: Option<EventDateTime>,
    end: Option<EventDateTime>,
    created: Option<String>,
    updated: Option<String>,
    organizer: Option<Organizer>,
    html_link: Option<String>,
    #[serde(default)]
    attendees: Vec<Attendee>,
    recurring_event_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct EventDateTime {
    date_time: Option<String>,
    date: Option<String>,
    time_zone: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Organizer {
    email: Option<String>,
    display_name: Option<String>,
    #[serde(rename = "self")]
    is_self: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Attendee {
    email: Option<String>,
    display_name: Option<String>,
    response_status: Option<String>,
    #[serde(rename = "self")]
    is_self: Option<bool>,
}

/// Opaque cursor we persist between syncs. Wraps either a Google
/// `syncToken` (for incremental) or nothing (for full sync).
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct CalendarCursor {
    /// Google's sync token for incremental sync.
    sync_token: Option<String>,
}

/// Fetch all pages of calendar events and return them as delta batches.
///
/// This is called from the connector's `sync()` method. It handles:
/// - Full sync: fetches events from `DEFAULT_LOOKBACK_DAYS` ago to now.
/// - Incremental sync: uses Google's `syncToken` for efficient updates.
/// - Pagination via `nextPageToken`.
/// - HTTP 410 → `SyncError::ResyncRequired`.
/// - HTTP 401/403 → `SyncError::AuthRequired`.
pub async fn fetch_calendar_events(
    http: &Client,
    access_token: &str,
    connector_id: &str,
    cursor: Option<SyncToken>,
    batch_size: usize,
) -> Result<Vec<DeltaBatch>, SyncError> {
    let prev_cursor: Option<CalendarCursor> = cursor
        .as_ref()
        .map(|c| serde_json::from_str(&c.0))
        .transpose()
        .map_err(|_| SyncError::ResyncRequired)?;

    let google_sync_token = prev_cursor.as_ref().and_then(|c| c.sync_token.clone());
    let is_incremental = google_sync_token.is_some();

    let mut all_events: Vec<CalendarEvent> = Vec::new();
    let mut page_token: Option<String> = None;
    let mut final_sync_token: Option<String> = None;

    loop {
        let mut url = format!(
            "{}/calendars/primary/events?maxResults={}&singleEvents=true&orderBy=startTime",
            calendar_api_base(), MAX_RESULTS_PER_PAGE
        );

        if let Some(ref sync_tok) = google_sync_token {
            // Incremental sync — Google ignores timeMin/timeMax/orderBy when
            // syncToken is provided, so we don't add them.
            if page_token.is_none() {
                url = format!(
                    "{}/calendars/primary/events?maxResults={}&syncToken={}",
                    calendar_api_base(),
                    MAX_RESULTS_PER_PAGE,
                    urlencoding::encode(sync_tok)
                );
            }
        } else {
            // Full sync — look back DEFAULT_LOOKBACK_DAYS.
            let time_min = (Utc::now() - chrono::Duration::days(DEFAULT_LOOKBACK_DAYS))
                .to_rfc3339();
            url.push_str(&format!("&timeMin={}", urlencoding::encode(&time_min)));
        }

        if let Some(ref pt) = page_token {
            url.push_str(&format!("&pageToken={}", urlencoding::encode(pt)));
        }

        debug!(url = %url, incremental = is_incremental, "fetching calendar events page");

        let response = http
            .get(&url)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|e| SyncError::Retryable {
                source: Box::new(e),
                retry_after: None,
            })?;

        let status = response.status();
        if status == reqwest::StatusCode::GONE {
            // 410: sync token expired. Discard and full-resync.
            warn!("Google Calendar returned 410 GONE — sync token expired");
            return Err(SyncError::ResyncRequired);
        }
        if status == reqwest::StatusCode::UNAUTHORIZED
            || status == reqwest::StatusCode::FORBIDDEN
        {
            return Err(SyncError::AuthRequired {
                reason: format!("Google Calendar API returned {status}"),
            });
        }
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(SyncError::Retryable {
                source: format!("Google Calendar API error {status}: {body}").into(),
                retry_after: None,
            });
        }

        let page: EventsListResponse =
            response.json().await.map_err(|e| SyncError::Retryable {
                source: Box::new(e),
                retry_after: None,
            })?;

        if let Some(events) = page.items {
            all_events.extend(events);
        }

        if let Some(nst) = page.next_sync_token {
            final_sync_token = Some(nst);
        }

        match page.next_page_token {
            Some(npt) => page_token = Some(npt),
            None => break,
        }
    }

    debug!(
        event_count = all_events.len(),
        incremental = is_incremental,
        "calendar events fetched"
    );

    // Convert to delta batches
    let cursor_value = CalendarCursor {
        sync_token: final_sync_token,
    };
    let cursor_token = SyncToken(
        serde_json::to_string(&cursor_value).expect("cursor serialization is infallible"),
    );

    let mut batches = Vec::new();
    for chunk in all_events.chunks(batch_size).collect::<Vec<_>>().into_iter() {
        let deltas: Vec<Delta> = chunk
            .iter()
            .map(|event| event_to_delta(connector_id, event))
            .collect();
        batches.push(DeltaBatch {
            connector_id: connector_id.to_string(),
            deltas,
            cursor: None, // intermediate batches don't carry a cursor
        });
    }

    // Last batch (or a synthetic empty one) carries the cursor.
    if let Some(last) = batches.last_mut() {
        last.cursor = Some(cursor_token);
    } else {
        // No events at all — still need to persist the sync token.
        batches.push(DeltaBatch {
            connector_id: connector_id.to_string(),
            deltas: Vec::new(),
            cursor: Some(cursor_token),
        });
    }

    Ok(batches)
}

/// Convert a single Calendar event to a Delta.
fn event_to_delta(connector_id: &str, event: &CalendarEvent) -> Delta {
    // Cancelled events are tombstones.
    if event.status.as_deref() == Some("cancelled") {
        return Delta::Tombstone {
            source_id: event.id.clone(),
        };
    }

    let timestamp = parse_event_start(event)
        .unwrap_or_else(Utc::now); // fallback shouldn't happen for valid events

    let properties = json!({
        "summary": event.summary,
        "description": event.description,
        "location": event.location,
        "start": event.start.as_ref().map(|s| json!({
            "dateTime": s.date_time,
            "date": s.date,
            "timeZone": s.time_zone,
        })),
        "end": event.end.as_ref().map(|e| json!({
            "dateTime": e.date_time,
            "date": e.date,
            "timeZone": e.time_zone,
        })),
        "organizer": event.organizer.as_ref().map(|o| json!({
            "email": o.email,
            "displayName": o.display_name,
            "self": o.is_self,
        })),
        "htmlLink": event.html_link,
        "attendees": event.attendees.iter().map(|a| json!({
            "email": a.email,
            "displayName": a.display_name,
            "responseStatus": a.response_status,
            "self": a.is_self,
        })).collect::<Vec<_>>(),
        "recurringEventId": event.recurring_event_id,
        "created": event.created,
        "updated": event.updated,
    });

    let mut item = Item::new(
        &event.id,
        connector_id,
        ItemKind::Event,
        timestamp,
        properties,
    );

    // Stash the raw Google response for traceability.
    item.raw_payload = serde_json::to_value(event).ok();

    Delta::Upsert(item)
}

/// Parse the event's start time. Prefer `dateTime`; fall back to `date`
/// (all-day events use date-only format).
fn parse_event_start(event: &CalendarEvent) -> Option<DateTime<Utc>> {
    let start = event.start.as_ref()?;
    if let Some(ref dt) = start.date_time {
        return DateTime::parse_from_rfc3339(dt)
            .ok()
            .map(|d| d.with_timezone(&Utc));
    }
    if let Some(ref d) = start.date {
        // All-day event: "2025-07-04". Parse as midnight UTC.
        return chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d")
            .ok()
            .and_then(|nd| nd.and_hms_opt(0, 0, 0))
            .map(|ndt| DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;

    #[test]
    fn cancelled_event_becomes_tombstone() {
        let event = CalendarEvent {
            id: "evt-1".into(),
            status: Some("cancelled".into()),
            summary: None,
            description: None,
            location: None,
            start: None,
            end: None,
            created: None,
            updated: None,
            organizer: None,
            html_link: None,
            attendees: Vec::new(),
            recurring_event_id: None,
        };
        match event_to_delta("google-calendar", &event) {
            Delta::Tombstone { source_id } => assert_eq!(source_id, "evt-1"),
            other => panic!("expected Tombstone, got {other:?}"),
        }
    }

    #[test]
    fn datetime_event_uses_start_time() {
        let event = CalendarEvent {
            id: "evt-2".into(),
            status: Some("confirmed".into()),
            summary: Some("Standup".into()),
            description: None,
            location: None,
            start: Some(EventDateTime {
                date_time: Some("2025-07-04T09:00:00-05:00".into()),
                date: None,
                time_zone: Some("America/Chicago".into()),
            }),
            end: Some(EventDateTime {
                date_time: Some("2025-07-04T09:30:00-05:00".into()),
                date: None,
                time_zone: Some("America/Chicago".into()),
            }),
            created: None,
            updated: None,
            organizer: None,
            html_link: None,
            attendees: Vec::new(),
            recurring_event_id: None,
        };

        match event_to_delta("google-calendar", &event) {
            Delta::Upsert(item) => {
                assert_eq!(item.source_id, "evt-2");
                assert_eq!(item.kind, ItemKind::Event);
                // 09:00 CDT = 14:00 UTC
                assert_eq!(item.timestamp.hour(), 14);
                assert_eq!(item.properties["summary"], "Standup");
            }
            other => panic!("expected Upsert, got {other:?}"),
        }
    }

    #[test]
    fn allday_event_parses_date_only() {
        let event = CalendarEvent {
            id: "evt-3".into(),
            status: Some("confirmed".into()),
            summary: Some("Independence Day".into()),
            description: None,
            location: None,
            start: Some(EventDateTime {
                date_time: None,
                date: Some("2025-07-04".into()),
                time_zone: None,
            }),
            end: Some(EventDateTime {
                date_time: None,
                date: Some("2025-07-05".into()),
                time_zone: None,
            }),
            created: None,
            updated: None,
            organizer: None,
            html_link: None,
            attendees: Vec::new(),
            recurring_event_id: None,
        };

        match event_to_delta("google-calendar", &event) {
            Delta::Upsert(item) => {
                assert_eq!(item.timestamp.date_naive().to_string(), "2025-07-04");
            }
            other => panic!("expected Upsert, got {other:?}"),
        }
    }

    #[test]
    fn deterministic_id_is_stable() {
        // Pin the expected ID so connector_id changes don't go unnoticed.
        let id = Item::deterministic_id("google-calendar", "evt-1");
        assert_eq!(id.to_string(), "3aaae9bc-36b9-540b-9158-3537f91fe703");
    }
}
