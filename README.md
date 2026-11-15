# PathwayDesigner website

This branch is the PathwayDesigner web app, ready to put on any web server.
Try it at https://sys-bio.github.io/PathwayDesigner-rs/

PathwayDesigner is a drawing tool for biochemical reaction networks. The source
code is on the `main` branch of https://github.com/sys-bio/PathwayDesigner-rs

## Hosting your own copy

Copy the files on this branch into a folder of your website, and open its
`index.html`, for example http://mysite.com/mysubdir/pathwaydesigner/index.html.
Any folder works. There is nothing to build or install, and no server-side code.

To get the files, either download this branch (Code ▸ Download ZIP on GitHub, with
the `gh-pages` branch selected), or clone it:

```
git clone --branch gh-pages --depth 1 https://github.com/sys-bio/PathwayDesigner-rs.git pathwaydesigner
```

Each GitHub release also has the same files as `PathwayDesigner-web-<version>.zip`.

The server needs to:
- serve `.wasm` files as `application/wasm` (almost every server does);
- use HTTPS, for working offline and for saving straight back to an opened file
  (plain HTTP also works, without those).

## Files

| File | What it is |
|---|---|
| `index.html` | The page |
| `pathway-designer-<hash>.js`, `pathway-designer-<hash>_bg.wasm` | The app, compiled from Rust to WebAssembly |
| `sw.js` | The service worker, which lets the app work offline |
| `THIRD-PARTY.html` | The licences of the open-source software the app is built from |

This branch is written by CI from each change to `main`; changes made to it
directly would be overwritten.

## Licence

MIT. See `THIRD-PARTY.html` for the licences of the software it includes.
