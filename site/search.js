// The documentation's search: Pagefind's interface, which the build's index
// feeds, filled by a query of `?q=`.
const search = new PagefindUI({ element: "#search", showSubResults: true, showImages: false });
const query = new URLSearchParams(window.location.search).get("q");
if (query) {
  search.triggerSearch(query);
}
