# Hosting NoteCraft for the web

`notecraft-web-<version>.zip` (from the GitHub release, or `packaging/web/package.sh`) holds a
static site in `notecraft-web-<version>/`:

| File | What it is |
|---|---|
| `index.html` | The page. It loads everything through relative URLs. |
| `notecraft-web-<hash>.js` | wasm-bindgen glue (generated, ES module) |
| `notecraft-web-<hash>_bg.wasm` | The app (several MB; much smaller with compression) |
| `_headers`, `.htaccess` | Sample header rules for Netlify/Cloudflare Pages and Apache |

There is no server-side code. Upload the folder's contents anywhere that serves static files.

## Any path works

All URLs in `index.html` are relative (`public_url = "./"` in `apps/notecraft-web/Trunk.toml`),
so the site works at a domain root (`https://example.com/`), under a prefix
(`https://example.com/tools/notecraft/`) and from a CDN bucket. The asset names carry a content
hash, so they can be cached forever. Only `index.html` needs revalidation.

## Required server settings

- **MIME type:** serve `.wasm` as `application/wasm`. Browsers refuse to stream-compile it under
  any other type, and the app then loads slowly or not at all. Serve `.js` as `text/javascript`.
  Most hosts already do both. For nginx, check that `mime.types` has `application/wasm wasm;`.
- **Compression:** turn on gzip or Brotli for `.wasm`, `.js` and `.html`. That roughly halves the
  download or better. You can also precompress (`brotli -k *.wasm`) and let
  the server send `Content-Encoding: br`.
- **Caching:** `Cache-Control: public, max-age=31536000, immutable` on the hashed `.wasm` and
  `.js` files, and `no-cache` on `index.html`.
- **HTTPS:** WebGPU (and the clipboard) only work in a secure context, which means `https://`
  or `http://localhost`. Over plain HTTP elsewhere, the app falls back to WebGL2.
- **No special isolation headers:** NoteCraft doesn't use `SharedArrayBuffer`, so it doesn't
  need `Cross-Origin-Opener-Policy` or `Cross-Origin-Embedder-Policy`. If your site already sends
  COEP `require-corp`, also send `Cross-Origin-Resource-Policy: same-origin` (or `cross-origin`
  when the files live on a CDN) on the app's files.

nginx example:

```nginx
location /notecraft/ {
    types { application/wasm wasm; text/javascript js; text/html html; }
    gzip on;
    gzip_types application/wasm text/javascript text/html;
    location ~* \.(wasm|js)$ { add_header Cache-Control "public, max-age=31536000, immutable"; }
    location ~* index\.html$ { add_header Cache-Control "no-cache"; }
}
```

Local test: `python3 -m http.server 8765` inside the folder, then open http://localhost:8765/.

## Embedding in a page (iframe)

```html
<iframe
  src="https://example.com/notecraft/"
  title="NoteCraft notebook"
  style="width: 100%; height: 720px; border: 0;"
  allow="fullscreen; clipboard-read; clipboard-write"
  allowfullscreen>
</iframe>
```

- The app fills the iframe and follows its size, so size the iframe and not the app.
- Keyboard shortcuts and pen input go to the iframe after the user clicks into it, as with any
  embedded app.
- **Sandboxed iframes** need at least
  `sandbox="allow-scripts allow-same-origin allow-downloads allow-popups"`. Without
  `allow-downloads`, exporting (`.onepkg`, `.one`, HTML, Markdown: browser downloads) is blocked.
- Don't send `X-Frame-Options: DENY` or a `frame-ancestors` CSP that excludes the embedding page.

## Where notebooks live

The web build has no file system. On its own, NoteCraft keeps the open notebooks in the browser
(`localStorage`, or IndexedDB once they outgrow it) and opens them again on the next visit. Export
a `.onepkg` (File › Export) to keep a copy elsewhere. The desktop apps autosave to real `.one` /
`.onetoc2` files instead.

## Hosting NoteCraft in your own page (host bridge)

A page can hand NoteCraft a notebook and store what it saves, for example to put notebooks in a
file manager or a web app's own storage. NoteCraft speaks the shared
[`craft-web-host`](https://github.com/storytold/craft-libs/tree/main/crates/web-host) contract,
the same messages as the other Craft apps' web builds, with the `notecraft:` prefix.

There are two ways to host it:

- **In an iframe:** add `?host=parent` to the `src`. The iframe must be on the **same origin** as
  the page: NoteCraft only talks to a same-origin parent. Messages go through `postMessage`.
  Without a parent, or with a parent on another origin, `?host=parent` is ignored and the app
  behaves as usual.
- **In the page's own document:** put `data-host="page"` on NoteCraft's canvas
  (`<canvas id="notecraft_canvas" data-host="page">`). Messages are `CustomEvent`s dispatched on
  the canvas, with the fields in `detail`.

When both are present, `?host=parent` wins.

| Message | Direction | Fields |
|---|---|---|
| `notecraft:ready` | app → host | `version` (the contract's, `"1"`), `appVersion` (NoteCraft's), `file: false` (send `bytes`) |
| `notecraft:open` | host → app | `name`, `bytes` (an `ArrayBuffer` or `Uint8Array`) |
| `notecraft:save` | app → host | `name`, `bytes` (an `ArrayBuffer`, transferred), `saveAs` |
| `notecraft:dirty` | app → host | `dirty` |
| `notecraft:failed` | app → host | `error` |

- **A notebook travels as one `.onepkg`.** Send `notecraft:open` with a `.onepkg` once you get
  `notecraft:ready`. (A single `.one` section or a NoteCraft `.json` opens too.) Files over
  512 MB are refused.
- NoteCraft has no Save button. Each autosave, a few seconds after a change, sends the whole
  notebook as a `.onepkg` in `notecraft:save`:
  - `saveAs: false` with the name you sent, for a notebook you sent as a `.onepkg`: overwrite
    that file.
  - `saveAs: true` with `<notebook name>.onepkg` the first time for a notebook you didn't send
    that way (one made with File › New, or a `.one` you sent). It is a new file. Later saves of
    it use the same name with `saveAs: false`.
  - File › Export's files (HTML, Markdown, plain text, a `.onepkg` copy) also come as
    `notecraft:save` with `saveAs: false`, instead of a browser download.
- `notecraft:dirty` reports when unsaved changes appear and when they are saved. The host should
  guard leaving the page while `dirty` is `true`. NoteCraft doesn't guard it in host mode.
- Hosted, NoteCraft keeps no notebooks in browser storage, and it starts with none until the host
  sends one. Its preferences (appearance, ribbon, language) are still kept per browser.

A minimal same-origin iframe host:

```html
<iframe id="nc" src="/notecraft/?host=parent" style="width: 100%; height: 720px; border: 0;"></iframe>
<script type="module">
  const frame = document.getElementById("nc");
  addEventListener("message", async (e) => {
    if (e.origin !== location.origin || e.source !== frame.contentWindow) return;
    const m = e.data;
    if (m.type === "notecraft:ready") {
      const bytes = new Uint8Array(await (await fetch("/files/Work.onepkg")).arrayBuffer());
      frame.contentWindow.postMessage({ type: "notecraft:open", name: "Work.onepkg", bytes }, location.origin);
    } else if (m.type === "notecraft:save") {
      await fetch(`/files/${encodeURIComponent(m.name)}`, { method: "PUT", body: m.bytes });
    } else if (m.type === "notecraft:dirty") {
      window.unsavedNotes = m.dirty;
    }
  });
</script>
```

## Renderer selection and fallback flags

NoteCraft renders with wgpu. It uses **WebGPU** when the browser has it and falls back to
**WebGL2** on its own. URL query flags override this, and they work on the iframe `src` too:

| Flag | Effect |
|---|---|
| *(none)* | WebGPU if available, otherwise WebGL2 |
| `?webgl` | Force the WebGL2 backend (useful when a WebGPU driver misbehaves) |

For example: `<iframe src="https://example.com/notecraft/?webgl" ...>`.

A browser with neither WebGPU nor WebGL2 gets a message in place of the app.
