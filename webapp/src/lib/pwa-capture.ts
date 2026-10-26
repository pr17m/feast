/** Runs before hydration (root layout, beforeInteractive): keep Chrome's install prompt for later. */
export const INSTALL_PROMPT_CAPTURE = `
window.addEventListener("beforeinstallprompt", function (e) {
  e.preventDefault();
  window.__feastInstallPrompt = e;
  window.dispatchEvent(new Event("feast-install-change"));
});
window.addEventListener("appinstalled", function () {
  window.__feastInstallPrompt = null;
  try { localStorage.setItem("feast.installed", "1"); } catch (e) {}
  window.dispatchEvent(new Event("feast-install-change"));
});`;
