# lokyr.dev

The sources of `lokyr.dev`: its home page, `public/`, and the documentation's site, which `cargo xtask site`, or `just site`, builds of the public Markdown into `target/site`, each document a page at its path, `.md` made `.html`.

- `public/`: the home page, one HTML file with its styles inline, the icon, and the y of the title, Playfair Display's, in `lokyr-y.woff2`, the glyph alone and renamed Lokyr Y, as the font's licence asks of a modified version; the licence is `lokyr-y-OFL.txt`. The numbers on the page are those of the latest release's session, in [`docs/bench/RELEASES.md`](../docs/bench/RELEASES.md); the icon is `crates/app/resources/icons/lokyr-jormungandr.svg`.
- `style.css`: the documentation's pages, in the home page's style.
- `site.toml`: the site's settings, the project's accounts that every page links.
- `search.js`: the search's page, of Pagefind's interface, which a query of `?q=` fills; Pagefind indexes the documents' pages as the site is built, where it is there, on the `PATH` or as `just pagefind` fetches it, of the release that the site pins, into `target/tools`.
- **Checks**: the build fails of a link of the pages, the home's too, to a file or an anchor that the site does not hold.
- **Preview**: `just site-serve` builds the site and serves it on `http://127.0.0.1:8124`. It loads nothing from elsewhere and sets no cookies; its one script is the search's.
- **Publish**: each push of `main` builds the site and publishes it on GitHub Pages at `https://lokyr.dev`, of the workflow `.github/workflows/site.yml`, with Pagefind's release for Linux that it pins. `public/appcast.xml`, the updates' feed that `https://lokyr.dev/appcast.xml` serves, is written by `just release` of each release, through `cargo xtask feed publish`, and committed once the release is published.
