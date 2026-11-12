# lokyr.dev

The temporary page of `lokyr.dev`, until the project's site: one HTML file with its styles inline, the icon, and an `.htaccess` that turns off the server's listing of directories. It loads nothing from elsewhere and sets no cookies: the y of the title is Playfair Display's, in `lokyr-y.woff2`, the glyph alone and renamed Lokyr Y, as the font's licence asks of a modified version; the licence is `lokyr-y-OFL.txt`.

- **Preview**: open `public/index.html` in a browser, or serve the folder, as `python3 -m http.server -d site/public`.
- **Publish**: `site/deploy.sh` shows the domain's document root, what it holds and what it would upload; `site/deploy.sh --upload` uploads `public/` through cPanel's UAPI and checks what the site then serves. It reads `LOKYR_CPANEL_HOST`, `LOKYR_CPANEL_USER` and `LOKYR_CPANEL_TOKEN` from the environment, or from `private/cpanel.env` in the main checkout, which git ignores; the token is an API token of the cPanel account, from cPanel › Security › Manage API Tokens.

The numbers on the page are those of the comparison in [`docs/bench/compare-F2-134.md`](../docs/bench/compare-F2-134.md); the icon is `crates/app/resources/icons/lokyr-jormungandr.svg`.
