#!/bin/sh
# Publishes site/public to the web host through cPanel's UAPI.
#
# It reads three variables, from the environment or else from
# private/cpanel.env in the main checkout, which git ignores:
#
#   LOKYR_CPANEL_HOST   the cPanel host, as lokyr.dev
#   LOKYR_CPANEL_USER   the cPanel account's user name
#   LOKYR_CPANEL_TOKEN  an API token of that account, from
#                       cPanel › Security › Manage API Tokens
#
# LOKYR_SITE_DOMAIN names the domain whose document root receives the files,
# lokyr.dev by default.
#
#   site/deploy.sh            shows the document root, what it holds now,
#                             and what would be uploaded
#   site/deploy.sh --upload   uploads, overwriting files of the same name,
#                             then checks what the site serves
#
# UAPI: https://api.docs.cpanel.net/cpanel/introduction/
set -eu

here=$(cd "$(dirname "$0")" && pwd)
public="$here/public"
common=$(git -C "$here" rev-parse --path-format=absolute --git-common-dir)
env_file=${LOKYR_CPANEL_ENV:-"$(dirname "$common")/private/cpanel.env"}
if [ -f "$env_file" ]; then
  set -a
  . "$env_file"
  set +a
fi
: "${LOKYR_CPANEL_HOST:?set it in the environment or in $env_file}"
: "${LOKYR_CPANEL_USER:?set it in the environment or in $env_file}"
: "${LOKYR_CPANEL_TOKEN:?set it in the environment or in $env_file}"
domain=${LOKYR_SITE_DOMAIN:-lokyr.dev}
api="https://$LOKYR_CPANEL_HOST:2083/execute"

# The token reaches curl through its standard input, so that no list of
# processes shows it.
uapi() {
  printf 'header = "Authorization: cpanel %s:%s"\n' \
    "$LOKYR_CPANEL_USER" "$LOKYR_CPANEL_TOKEN" |
    curl --silent --show-error --fail --max-time 120 --config - "$@"
}

# UAPI answers errors with status 0 and HTTP 200: this fails on them, and
# prints the field of `data` that its argument names, as a Python expression.
field() {
  python3 -I -c '
import json, sys
r = json.load(sys.stdin)
if r.get("status") != 1:
    sys.exit("cPanel: " + "; ".join(r.get("errors") or ["unknown error"]))
data = r["data"]
print(eval(sys.argv[1]))
' "$1"
}

if find "$public" -mindepth 1 -type d | grep -q .; then
  echo "site/public holds directories, which this script does not upload" >&2
  exit 1
fi

docroot=$(uapi "$api/DomainInfo/single_domain_data?domain=$domain" |
  field 'data["documentroot"]')
echo "Document root of $domain: $docroot"

echo "It holds:"
uapi --get --data-urlencode "dir=$docroot" "$api/Fileman/list_files" |
  field '"\n".join("  " + f["file"] + ("/" if f["type"] == "dir" else "") for f in data) or "  nothing"'

echo "To upload, from site/public:"
for f in "$public"/* "$public"/.[!.]*; do
  [ -f "$f" ] && echo "  $(basename "$f")"
done

if [ "${1:-}" != "--upload" ]; then
  echo "Run with --upload to publish."
  exit 0
fi

set -- -F "dir=$docroot" -F overwrite=1
n=0
for f in "$public"/* "$public"/.[!.]*; do
  [ -f "$f" ] || continue
  n=$((n + 1))
  set -- "$@" -F "file-$n=@$f"
done
uapi "$@" "$api/Fileman/upload_files" |
  field '"Uploaded %d, failed %d" % (data["succeeded"], data["failed"]) if not data["failed"] else sys.exit("cPanel: " + "; ".join(u["reason"] for u in data["uploads"] if not u["status"]))'

# What the site serves now, against what was uploaded.
if curl --silent --show-error --fail --max-time 30 "https://$domain/index.html" |
  cmp -s - "$public/index.html"; then
  echo "https://$domain/ serves the new index.html"
else
  echo "https://$domain/ does not serve the new index.html yet" >&2
  exit 1
fi
