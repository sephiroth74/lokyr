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

// The benches' charts of the releases answer the pointer and the keyboard:
// a release under the pointer, or of the arrows once the chart has the
// focus, is marked in every panel of its chart, and a card tells its values
// in the panel, each one's change from the release before and its share of
// the budget; a click, or Return, opens the release's report; a legend's
// item turns its series off and on.
(function () {
  var count = 0;
  document.querySelectorAll('.doc svg.chart[data-chart="trends"]').forEach(function (svg) {
    var panels = Array.prototype.slice.call(svg.querySelectorAll("g.panel"));
    if (!panels.length) return;
    var releases = Array.prototype.map.call(panels[0].querySelectorAll("rect.column"), function (column) {
      return { label: column.getAttribute("data-release"), report: column.getAttribute("data-report") };
    });
    if (!releases.length) return;
    var card = document.createElement("div");
    card.className = "chart-card";
    card.id = "chart-card-" + ++count;
    card.hidden = true;
    card.setAttribute("role", "status");
    svg.parentElement.appendChild(card);
    svg.setAttribute("tabindex", "0");
    svg.setAttribute("role", "group");
    svg.setAttribute("aria-describedby", card.id);
    var at = { panel: 0, index: releases.length - 1 };

    function amount(value, digits, unit) {
      var text = Math.abs(value).toLocaleString("en-US", {
        minimumFractionDigits: digits, maximumFractionDigits: digits,
      });
      return !unit ? text : unit.charAt(0) === "/" ? text + unit : text + " " + unit;
    }

    function line(parent, className, text) {
      var element = document.createElement("div");
      element.className = className;
      element.textContent = text;
      parent.appendChild(element);
      return element;
    }

    function show(panelIndex, index) {
      at = { panel: panelIndex, index: index };
      var release = releases[index];
      svg.querySelectorAll("rect.column").forEach(function (column) {
        column.classList.toggle("on", column.getAttribute("data-release") === release.label);
      });
      var panel = panels[panelIndex];
      var digits = Number(panel.getAttribute("data-digits") || 1);
      var unit = panel.getAttribute("data-unit") || "";
      var budget = panel.getAttribute("data-budget");
      var names = (panel.getAttribute("data-names") || "").split("|");
      card.textContent = "";
      line(card, "chart-card-title", release.label + " · " + panel.getAttribute("data-title"));
      var told = false;
      [0, 1].forEach(function (series) {
        if (svg.classList.contains("hide-" + series)) return;
        var dots = Array.prototype.slice.call(panel.querySelectorAll('circle[data-series="' + series + '"]'));
        var dot = dots.filter(function (d) { return d.getAttribute("data-release") === release.label; })[0];
        if (!dot) return;
        told = true;
        var value = Number(dot.getAttribute("data-value"));
        var name = names.length > 1 && names[series] ? names[series] + ": " : "";
        line(card, "chart-card-value", name + amount(value, digits, unit));
        var labels = releases.map(function (r) { return r.label; });
        var before = dots.filter(function (d) {
          return labels.indexOf(d.getAttribute("data-release")) < index;
        }).pop();
        if (before) {
          var previous = Number(before.getAttribute("data-value"));
          var change = value - previous;
          var sign = change > 0 ? "+" : change < 0 ? "−" : "±";
          var share = previous ? " (" + sign + Math.abs(change / previous * 100).toFixed(0) + " %)" : "";
          line(card, "chart-card-change", sign + amount(change, digits, unit) + share + " from " +
            before.getAttribute("data-release"));
        }
        if (budget !== null) {
          line(card, "chart-card-budget", panel.getAttribute("data-better") === "higher"
            ? "Budget ≥ " + amount(Number(budget), digits, unit)
            : Math.round(value / Number(budget) * 100) + " % of the budget, " + amount(Number(budget), digits, unit));
        }
      });
      if (!told) line(card, "chart-card-change", "Not measured");
      if (release.report) {
        line(card, "chart-card-report", "Click or press Return for its report, " +
          release.report.replace(/\.md$/, ""));
      }
      card.hidden = false;
      // Beside the release's column, inside the figure.
      var figure = svg.parentElement.getBoundingClientRect();
      var column = panel.querySelector('rect.column[data-index="' + index + '"]').getBoundingClientRect();
      var left = column.right - figure.left + 8;
      if (left + card.offsetWidth > figure.width) left = column.left - figure.left - card.offsetWidth - 8;
      card.style.left = Math.max(0, left) + "px";
      card.style.top = Math.max(0, column.top - figure.top) + "px";
    }

    function hide() {
      card.hidden = true;
      svg.querySelectorAll("rect.column.on").forEach(function (column) { column.classList.remove("on"); });
    }

    function open(index) {
      var report = releases[index] && releases[index].report;
      if (report) window.location.href = report.replace(/\.md$/, ".html");
    }

    svg.addEventListener("pointermove", function (event) {
      var column = event.target.closest ? event.target.closest("rect.column") : null;
      if (!column) return hide();
      show(panels.indexOf(column.closest("g.panel")), Number(column.getAttribute("data-index")));
    });
    svg.addEventListener("pointerleave", function () {
      if (document.activeElement !== svg) hide();
    });
    svg.addEventListener("click", function (event) {
      var column = event.target.closest ? event.target.closest("rect.column") : null;
      if (column) open(Number(column.getAttribute("data-index")));
    });
    svg.addEventListener("focus", function () { show(at.panel, at.index); });
    svg.addEventListener("blur", hide);
    svg.addEventListener("keydown", function (event) {
      if (event.target !== svg) return;
      var panel = at.panel, index = at.index;
      switch (event.key) {
        case "ArrowLeft": index = Math.max(0, index - 1); break;
        case "ArrowRight": index = Math.min(releases.length - 1, index + 1); break;
        case "ArrowUp": panel = Math.max(0, panel - 1); break;
        case "ArrowDown": panel = Math.min(panels.length - 1, panel + 1); break;
        case "Home": index = 0; break;
        case "End": index = releases.length - 1; break;
        case "Enter": open(index); return;
        case "Escape": hide(); return;
        default: return;
      }
      event.preventDefault();
      show(panel, index);
    });
    svg.querySelectorAll("g.legend-item").forEach(function (item) {
      var series = item.getAttribute("data-series");
      item.setAttribute("tabindex", "0");
      item.setAttribute("role", "button");
      item.setAttribute("aria-pressed", "true");
      function toggle() {
        var hidden = svg.classList.toggle("hide-" + series);
        item.setAttribute("aria-pressed", hidden ? "false" : "true");
        if (!card.hidden) show(at.panel, at.index);
      }
      item.addEventListener("click", function (event) { event.stopPropagation(); toggle(); });
      item.addEventListener("keydown", function (event) {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          toggle();
        }
      });
    });
  });
})();

// A wide page's tables: a group's row of its title alone marked; a sign of
// a verdict, ▲, ▼ or ✖, of its color; a measure not taken dimmed; the
// column under the pointer marked, and the release of its header in the
// page's charts with it.
(function () {
  var marks = { "▲": "mark-better", "▼": "mark-worse", "✖": "mark-over" };
  document.querySelectorAll(".page.wide .doc table").forEach(function (table) {
    table.querySelectorAll("tbody tr").forEach(function (row) {
      var cells = Array.prototype.slice.call(row.children);
      var first = cells[0];
      var title = first && first.children.length === 1 && first.firstElementChild.tagName === "STRONG" &&
        first.textContent.trim() === first.firstElementChild.textContent.trim();
      if (title && cells.slice(1).every(function (cell) { return !cell.textContent.trim(); })) {
        row.classList.add("group");
      }
      cells.forEach(function (cell) {
        if (/^—/.test(cell.textContent.trim())) cell.classList.add("none");
        var walker = document.createTreeWalker(cell, NodeFilter.SHOW_TEXT);
        var texts = [];
        while (walker.nextNode()) texts.push(walker.currentNode);
        texts.forEach(function (text) {
          if (!/[▲▼✖]/.test(text.data)) return;
          var parts = text.data.split(/([▲▼✖])/);
          var fragment = document.createDocumentFragment();
          parts.forEach(function (part) {
            if (marks[part]) {
              var span = document.createElement("span");
              span.className = marks[part];
              span.textContent = part;
              fragment.appendChild(span);
            } else if (part) {
              fragment.appendChild(document.createTextNode(part));
            }
          });
          text.parentNode.replaceChild(fragment, text);
        });
      });
    });
    var headers = Array.prototype.slice.call(table.querySelectorAll("thead th"));
    var marked = -1;
    function mark(index) {
      if (index === marked) return;
      marked = index;
      table.querySelectorAll(".column-on").forEach(function (cell) { cell.classList.remove("column-on"); });
      var label = index > 0 && headers[index] ? headers[index].textContent.trim() : "";
      if (index > 0) {
        table.querySelectorAll("tr").forEach(function (row) {
          if (row.classList.contains("group")) return;
          var cell = row.children[index];
          if (cell) cell.classList.add("column-on");
        });
      }
      document.querySelectorAll(".doc svg.chart rect.column").forEach(function (column) {
        column.classList.toggle("on", label !== "" && column.getAttribute("data-release") === label);
      });
    }
    table.addEventListener("pointerover", function (event) {
      var cell = event.target.closest ? event.target.closest("td, th") : null;
      if (cell && table.contains(cell)) mark(cell.cellIndex);
    });
    table.addEventListener("pointerleave", function () { mark(-1); });
  });
})();
