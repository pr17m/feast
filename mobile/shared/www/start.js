/*
 * Feast app start page (bundled in the app, works offline).
 *
 * Picks the country deployment once, remembers it (Capacitor Preferences, with
 * localStorage as a fallback) and opens that country's live site. Opening the
 * app from a Feast link (universal link / app link / custom scheme) goes
 * straight to that page. `index.html?change=1` forgets the choice and shows
 * the picker again (the web app's "Change country" uses this).
 */
(function () {
  "use strict";
  var cfg = window.FEAST_APP || {};
  var countries = cfg.countries || [];
  var KEY = "feast.country";
  var cap = window.Capacitor;
  var hasNative = !!(cap && typeof cap.nativePromise === "function" && cap.isPluginAvailable && cap.isPluginAvailable("Preferences"));

  function $(id) {
    return document.getElementById(id);
  }
  function show(id) {
    ["picker", "opening", "not-configured"].forEach(function (s) {
      $(s).hidden = s !== id;
    });
  }

  function load() {
    if (hasNative) {
      return cap.nativePromise("Preferences", "get", { key: KEY }).then(
        function (r) {
          return r && r.value;
        },
        function () {
          return null;
        },
      );
    }
    try {
      return Promise.resolve(window.localStorage.getItem(KEY));
    } catch (e) {
      return Promise.resolve(null);
    }
  }
  function save(code) {
    try {
      window.localStorage.setItem(KEY, code);
    } catch (e) {
      /* ignore */
    }
    return hasNative ? cap.nativePromise("Preferences", "set", { key: KEY, value: code }).catch(function () {}) : Promise.resolve();
  }
  function forget() {
    try {
      window.localStorage.removeItem(KEY);
    } catch (e) {
      /* ignore */
    }
    return hasNative ? cap.nativePromise("Preferences", "remove", { key: KEY }).catch(function () {}) : Promise.resolve();
  }

  function configured(c) {
    return !!c && /^https:\/\/[^/]+/.test(c.url || "") && !/\.example(\/|$)/.test(c.url);
  }
  function byCode(code) {
    for (var i = 0; i < countries.length; i++) if (countries[i].code === code) return countries[i];
    return null;
  }
  function byHost(host) {
    for (var i = 0; i < countries.length; i++) {
      try {
        if (new URL(countries[i].url).host === host) return countries[i];
      } catch (e) {
        /* ignore */
      }
    }
    return null;
  }

  function open(c, path) {
    if (!configured(c)) {
      $("nc-country").textContent = c ? c.name : "this country";
      show("not-configured");
      return;
    }
    $("opening-country").textContent = c.name;
    show("opening");
    var base = c.url.replace(/\/+$/, "");
    var p = path && path.charAt(0) === "/" ? path : cfg.startPath || "/";
    window.location.replace(base + p);
  }

  function renderPicker() {
    var list = $("countries");
    list.innerHTML = "";
    countries.forEach(function (c) {
      var li = document.createElement("li");
      var b = document.createElement("button");
      b.type = "button";
      b.className = "country";
      b.innerHTML = '<span class="flag" aria-hidden="true"></span><span class="name"></span>';
      b.querySelector(".flag").textContent = c.flag || "";
      b.querySelector(".name").textContent = c.name;
      b.addEventListener("click", function () {
        save(c.code).then(function () {
          open(c);
        });
      });
      li.appendChild(b);
      list.appendChild(li);
    });
    show("picker");
  }

  /** The Feast link the app was opened with, if any (cold start). */
  function launchLink() {
    if (!cap || typeof cap.nativePromise !== "function" || !cap.isPluginAvailable || !cap.isPluginAvailable("App")) return Promise.resolve(null);
    return cap.nativePromise("App", "getLaunchUrl", {}).then(
      function (r) {
        return r && r.url ? r.url : null;
      },
      function () {
        return null;
      },
    );
  }

  $("app-name").textContent = cfg.appName || "Feast";
  $("tagline").textContent = cfg.tagline || "";
  document.title = cfg.appName || "Feast";
  $("nc-back").addEventListener("click", function () {
    forget().then(renderPicker);
  });

  var params = new URLSearchParams(window.location.search);
  if (params.get("change") === "1") {
    forget().then(renderPicker);
    return;
  }

  Promise.all([load(), launchLink()]).then(function (res) {
    var saved = byCode(res[0]);
    var link = res[1];
    if (link) {
      try {
        var u = new URL(link);
        // https://<country host>/r/abc → that country; feast://r/abc → the saved country
        var fromHost = u.protocol === "https:" ? byHost(u.host) : null;
        var path = u.protocol === "https:" ? u.pathname + u.search : "/" + (u.host + u.pathname).replace(/^\/+/, "") + u.search;
        var target = fromHost || saved;
        if (target) {
          if (fromHost && fromHost !== saved) save(fromHost.code);
          open(target, path);
          return;
        }
      } catch (e) {
        /* fall through */
      }
    }
    if (saved) open(saved);
    else renderPicker();
  });
})();
