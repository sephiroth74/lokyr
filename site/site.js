// The documentation's pages, a little more where a script runs; each works
// as well without it.

// On a phone, the top bar's row of sections scrolls to show the current one,
// the row alone, never the page.
(function () {
  var current = document.querySelector(".top nav [aria-current]");
  if (!current) return;
  var row = current.parentElement;
  var shown = current.getBoundingClientRect();
  var box = row.getBoundingClientRect();
  if (shown.left < box.left || shown.right > box.right) {
    row.scrollLeft += shown.left - box.left - (box.width - shown.width) / 2;
  }
})();

// The table of contents follows the reading: the part in view is marked,
// and its section's group opened, which closes again as the reading leaves
// it unless it was opened by hand.
(function () {
  var toc = document.querySelector(".toc");
  if (!toc) return;
  var links = new Map();
  toc.querySelectorAll('a[href^="#"]').forEach(function (link) {
    links.set(decodeURIComponent(link.hash.slice(1)), link);
  });
  var headings = Array.prototype.filter.call(
    document.querySelectorAll(".doc h2[id], .doc h3[id]"),
    function (heading) { return links.has(heading.id); }
  );
  if (!headings.length) return;
  var current = null;
  var opened = null;
  function mark(id) {
    if (id === current) return;
    current = id;
    var marked = toc.querySelector("a[aria-current]");
    if (marked) marked.removeAttribute("aria-current");
    var link = links.get(id);
    link.setAttribute("aria-current", "location");
    var group = link.closest("details");
    if (opened && opened !== group) opened.open = false;
    opened = null;
    if (group && !group.open) {
      group.open = true;
      opened = group;
    }
    var shown = link.getBoundingClientRect();
    var box = toc.getBoundingClientRect();
    if (shown.top < box.top || shown.bottom > box.bottom) {
      toc.scrollTop += shown.top - box.top - box.height / 3;
    }
  }
  // The part whose heading last passed a third of the way down the screen,
  // or under the top bar where the screen is short.
  var bar = parseFloat(getComputedStyle(document.documentElement).scrollPaddingTop) || 88;
  var waiting = false;
  function look() {
    waiting = false;
    var found = headings[0];
    var line = Math.max(bar, innerHeight / 3);
    for (var i = 0; i < headings.length; i++) {
      if (headings[i].getBoundingClientRect().top - line > 1) break;
      found = headings[i];
    }
    mark(found.id);
  }
  addEventListener("scroll", function () {
    if (!waiting) {
      waiting = true;
      requestAnimationFrame(look);
    }
  }, { passive: true });
  look();
})();
