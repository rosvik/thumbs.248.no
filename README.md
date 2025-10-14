# thumbs.248.no

This is a simple proxy for YouTube thumbnails. It will fetch the best quality thumbnail for a given YouTube video, and cache them. YouTube servers will be queried only the first time a thumbnail is requested.

Go to [thumbs.248.no](https://thumbs.248.no) and enter a YouTube URL or video ID to see it in action.

## Redirector setup

Using Redirector (a browser extension you can download from [here](https://einaregilsson.com/redirector/)), you can redirect any YouTube thumbnail URL to the proxy. This will use the best resolution available for _all_ thumbnails on youtube.com, so expect a bit of extra data usage.

You can import [this predefined rule](./docs/Redirector.json) or manually create a rule with the following settings:

- Include pattern: `https://*.ytimg.com/vi*/*/*`
- Redirect to: `https://thumbs.248.no/$3`
- In advanced options, check every box under *Apply to* except "Main window (address bar)"

<div align="right"><img src="https://github-production-user-asset-6210df.s3.amazonaws.com/1774972/269361517-d0d8e30e-4a25-4ba2-b926-2a42da1156f8.svg" width="32" alt="248"></div>
