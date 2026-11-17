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

// Each code block takes a button that copies its text, which tells that it
// copied for a moment; where the clipboard is not the page's, as over plain
// HTTP, the button does not come.
(function () {
  if (!navigator.clipboard || !window.isSecureContext) return;
  document.querySelectorAll(".doc pre").forEach(function (pre) {
    var code = pre.querySelector("code") || pre;
    var box = document.createElement("div");
    box.className = "code";
    pre.parentNode.insertBefore(box, pre);
    box.appendChild(pre);
    var button = document.createElement("button");
    button.type = "button";
    button.className = "copy";
    button.textContent = "Copy";
    button.setAttribute("aria-label", "Copy the code");
    var timer = null;
    button.addEventListener("click", function () {
      navigator.clipboard.writeText(code.innerText.replace(/\n$/, "")).then(function () {
        button.textContent = "Copied";
        button.setAttribute("data-copied", "");
        clearTimeout(timer);
        timer = setTimeout(function () {
          button.textContent = "Copy";
          button.removeAttribute("data-copied");
        }, 1500);
      });
    });
    box.appendChild(button);
  });
})();
