# firefly-api

This repo hosts serverless APIs used by the firefly project hosted on cloudflare workers.

## Development

The project needs to interact with [spotify web API](https://developer.spotify.com/documentation/web-api). Add the client_id and secret to `.dev.vars` file under your project root:

```
SPOTIFY_WEB_API_CLIENT_ID=<your client ID>
SPOTIFY_WEB_API_CLIENT_SECRET=<your secret>
```

Then run
```
npx wrangler dev
```
