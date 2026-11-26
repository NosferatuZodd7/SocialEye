# Platform limitations

SocialEye only uses official APIs. These are the limits that come from the platforms, and what SocialEye does instead. The same information appears in the app under **Accounts → What this platform allows** (the capability matrix in `src-tauri/src/connectors/mod.rs`).

## Instagram
- **Follower and following lists are not available through any API.** Use the data export import (JSON or HTML) to compare followers, find accounts that don't follow back, and track new followers/unfollowers between exports.
- Only Professional (Creator/Business) accounts linked to a Facebook Page.
- Insights: reach and daily follower change; `follower_count` covers the last 30 days and needs 100+ followers. Meta regularly retires metrics — anything that fails is listed as unavailable after a sync.
- Publishing requires media at a public HTTPS URL and App Review; 50 API posts per 24 hours.
- No scheduling API: SocialEye publishes scheduled Instagram posts itself, **only while the app is running**. If it was closed and the time passed by more than 15 minutes, the post is marked *failed (missed schedule)* and never published late without you.
- Unfollowing, DMs and stories are not automated.

## Facebook
- Personal profiles cannot be read or published to via the API (removed by Meta in 2018). SocialEye works with Pages.
- Page insights metric names change over time; missing ones are reported per sync.
- Photo posts and text/link posts are supported; videos should be uploaded in Meta Business Suite.
- Native scheduling between 10 minutes and 30 days ahead.

## YouTube
- No Shorts flag in the API: videos ≤ 3 minutes are labelled "Short (probable)".
- Impressions and impressions CTR are not in the Analytics API.
- Unaudited API projects can only upload private videos; custom thumbnails need a verified channel.
- Channel owners cannot hard-delete viewers' comments through the API; "delete" is not offered — use reject/hold for review.
- Daily quota (default 10,000 units).

## Spotify
- Public Web API: catalogue only. Plays, listeners, followers, consumption and demographics come from Spotify for Creators CSV exports that you import.
- No public publishing endpoint.
- The CSV importer recognises common headers (Date, Plays/Streams, Starts, Listeners, Followers, Episode). Slash dates are read as month/day/year unless the first number is above 12.

## General
- Error messages from platform APIs are shown as returned (usually in English).
- Desktop notifications and scheduled publishing only run while SocialEye is open (it has no background service).
- Installers are unsigned unless a code-signing certificate is configured in the release workflow, so Windows SmartScreen may warn on first run.
