# API and OAuth setup

SocialEye has no central server and ships **no API keys**. You create a developer app with each provider and paste its credentials in **Settings → Platform apps**. Client secrets are stored in Windows Credential Manager, never in the database or exports.

All providers use the OAuth 2.0 authorization-code flow with PKCE and a loopback redirect:

```
http://127.0.0.1:<port>/callback        (default port 53682, configurable per provider)
```

Register exactly this URI (with your port) where the provider asks for redirect URIs.

## Summary

| Platform | Provider console | Account requirement | Review needed for… |
|---|---|---|---|
| YouTube | Google Cloud Console | Any Google account with a YouTube channel | Public use beyond your test users (OAuth verification of sensitive scopes); public uploads by unaudited projects |
| Facebook | Meta for Developers | A Facebook **Page** where you have a task | Anyone who is not an admin/developer/tester of your app (App Review + Business Verification) |
| Instagram | Meta for Developers (same app) | Instagram **Professional** (Creator/Business) account linked to a Page | Same as Facebook; `instagram_content_publish` needs App Review |
| Spotify | Spotify Developer Dashboard | Any Spotify account | Extended quota mode for more than the development-mode user allow-list |

## Google (YouTube)

1. Open <https://console.cloud.google.com/>, create a project.
2. **APIs & Services → Library**: enable **YouTube Data API v3** and **YouTube Analytics API**.
3. **OAuth consent screen**: User type *External*. Add the scopes
   `youtube.readonly`, `yt-analytics.readonly`, `youtube.upload`, `youtube.force-ssl`.
   While the app is in *Testing*, add your Google account under **Test users**.
4. **Credentials → Create credentials → OAuth client ID → Application type: Desktop app**.
   Desktop clients accept any loopback port automatically.
5. Copy the **Client ID** and **Client secret** into SocialEye (Google requires the secret for desktop clients; it is not confidential for installed apps but SocialEye still keeps it in the credential store).

Notes
- In *Testing* status Google refresh tokens expire after 7 days — you will be asked to reconnect weekly until the app is published.
- Projects that have not passed a YouTube API compliance audit can only upload videos as **private**.
- The Data API has a daily quota (10,000 units by default). Uploads cost far more than reads; SocialEye limits each sync to the 250 most recent uploads.
- Impressions and impressions CTR are not in the YouTube Analytics API (they are only in bulk Reporting API jobs), so SocialEye shows them as unavailable.

## Meta (Facebook Pages and Instagram)

One Meta app serves both platforms.

1. Open <https://developers.facebook.com/apps/> → **Create app** → choose the *Business* type (or the use case "Manage everything on your Page").
2. Add **Facebook Login for Business** (or Facebook Login). In its settings add the redirect URI `http://127.0.0.1:53682/callback` under **Valid OAuth Redirect URIs**.
3. Permissions SocialEye requests:
   - Facebook: `pages_show_list`, `pages_read_engagement`, `pages_manage_posts`, `pages_manage_engagement`, `read_insights`, `business_management`
   - Instagram: `instagram_basic`, `instagram_manage_insights`, `instagram_manage_comments`, `instagram_content_publish`, plus `pages_show_list`, `pages_read_engagement`, `business_management`
4. **App settings → Basic**: copy **App ID** (Client ID) and **App Secret**.
5. Optional: set the Graph API version in SocialEye (default `v24.0`). Meta retires versions about two years after release.

Notes
- Without the App Secret SocialEye cannot exchange for long-lived tokens, so Page tokens expire after about an hour. With it, Page tokens do not expire unless you change your password or remove the app.
- In **Development mode** only people with a role on the app can connect. To let others use your app you need App Review for each permission and Business Verification.
- Instagram: convert the account to Creator or Business (Instagram app → Settings → Account type), then link it to a Facebook Page (Page settings → Linked accounts). Personal accounts are not supported by the API.
- Instagram publishing needs the media at a **public HTTPS URL** (Meta downloads it). Set it on the media item in the Media library.

## Spotify

1. Open <https://developer.spotify.com/dashboard> → **Create app**.
2. Redirect URI: `http://127.0.0.1:53682/callback` (Spotify requires the `127.0.0.1` literal, not `localhost`). API: **Web API**.
3. Copy the **Client ID**. No secret is needed — SocialEye uses PKCE.
4. In *Development mode* add the Spotify accounts that will connect under **User Management**.

Notes
- The Web API exposes podcast catalogue data only. Plays, listeners, followers and demographics are only in Spotify for Creators. Download the analytics CSV there and use **Spotify → Import CSV** in SocialEye.
- There is no public podcast publishing API. SocialEye links to <https://creators.spotify.com>.

## Changing the redirect port

If port 53682 is in use, pick another (1024–65535) in Settings and register the new redirect URI with the provider (Google desktop clients need no change).
