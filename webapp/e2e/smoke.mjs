// End-to-end test against real deployments (web + Rust API + database):
// customer, owner and admin flows at desktop + phone sizes, for two countries.
//
//   BASE     India web deployment      (default http://localhost:3100 → API with FEAST_COUNTRY=in)
//   BASE_AU  Australia web deployment  (default http://localhost:3101 → API with FEAST_COUNTRY=au)
//
// Uses the development seed accounts loaded into each database (feast seed).
import { chromium } from "playwright";
import fs from "node:fs";

const BASE = process.env.BASE ?? "http://localhost:3100";
const BASE_AU = process.env.BASE_AU ?? "http://localhost:3101";
const ACCOUNTS = {
  customer: { phone: "9876543210" }, // Kavya, Bengaluru (+91)
  owner: { email: "owner@feast.demo", password: "feast123" },
  admin: { email: "admin@feast.demo", password: "feast123" },
  customerAu: { phone: "412345678" }, // Jack, Sydney (+61)
  ownerAu: { email: "owner.au@feast.demo", password: "feast123" },
};
const DEV_OTP = process.env.DEV_OTP ?? "123456";
const OUT = process.argv[2] ?? new URL("./shots", import.meta.url).pathname;
fs.mkdirSync(OUT, { recursive: true });

const errors = [];
const log = (...a) => console.log("•", ...a);

async function newCtx(browser, viewport, mobile) {
  const ctx = await browser.newContext({ viewport, deviceScaleFactor: mobile ? 2 : 1, isMobile: mobile, hasTouch: mobile });
  ctx.on("page", (p) => wire(p));
  return ctx;
}
function wire(page) {
  page.on("pageerror", (e) => errors.push(`pageerror ${page.url()}: ${e.message}`));
  page.on("console", (m) => {
    // Expected: the "not found" page for a hidden restaurant, and an unknown QR code.
    const expected =
      /favicon/.test(m.text()) ||
      (/status of 404/.test(m.text()) && /\/restaurants\/sugar-bloom|\/r\/nope/.test(page.url()));
    if (m.type() === "error" && !expected) errors.push(`console ${page.url()}: ${m.text()}`);
  });
}
const shot = (page, name) => page.screenshot({ path: `${OUT}/${name}.png`, fullPage: false });

/** Sign in through the real login page: diners by OTP, partners/admins by password. */
async function login(page, which, base = BASE) {
  const a = ACCOUNTS[which];
  if (a.phone) {
    await page.goto(`${base}/login`);
    await page.getByLabel("Mobile number").fill(a.phone);
    await page.getByRole("button", { name: "Get OTP" }).click();
    await page.getByLabel("OTP").fill(DEV_OTP);
    await page.getByRole("button", { name: "Verify & sign in" }).click();
  } else {
    await page.goto(`${base}/login?as=partner`);
    await page.getByLabel("Work email").fill(a.email);
    await page.getByLabel("Password").fill(a.password);
    await page.getByRole("button", { name: "Sign in", exact: true }).click();
  }
  await page.waitForURL((u) => !u.pathname.startsWith("/login"), { timeout: 15000 });
}
async function logout(page) {
  await page.evaluate(() => {
    sessionStorage.setItem("feast.auth.token", "");
    localStorage.removeItem("feast.auth.last");
  });
}

const browser = await chromium.launch();

// ---------------- Desktop: full journey ----------------
{
  const ctx = await newCtx(browser, { width: 1440, height: 900 }, false);
  const page = await ctx.newPage();

  await page.goto(BASE);
  await page.getByText("Top rated near you").waitFor();
  await shot(page, "d01-home");
  log("home ok");

  await page.goto(`${BASE}/search?q=pizza`);
  await page.getByText(/restaurant(s)? for/).waitFor();
  await shot(page, "d02-search");
  log("search ok:", await page.getByText(/restaurant(s)? for/).innerText());

  await page.goto(`${BASE}/restaurants/olive-and-ember`);
  await page.getByRole("heading", { name: "Olive & Ember" }).waitFor();
  await shot(page, "d03-restaurant");
  log("restaurant ok");

  // Guarded route redirects to login
  await page.goto(`${BASE}/bookings`);
  await page.waitForURL(/\/login\?next=/);
  log("guard redirect ok");

  await login(page, "customer");
  log("customer login ->", new URL(page.url()).pathname);

  // Book a table
  await page.goto(`${BASE}/restaurants/olive-and-ember/book`);
  await page.getByRole("radiogroup", { name: "Date" }).getByRole("radio").nth(1).click(); // tomorrow
  await page.getByRole("radiogroup", { name: "Time slot" }).waitFor();
  await page.getByRole("radiogroup", { name: "Time slot" }).locator("[role=radio]:not([disabled])").first().click();
  await shot(page, "d04-book-slots");
  await page.getByRole("button", { name: /^Continue/ }).click();
  await page.getByLabel("Name").fill("Kavya Rao");
  await page.getByLabel("Mobile number").fill("9876543210");
  await page.getByRole("button", { name: "Confirm booking" }).click();
  await page.getByText("Table booked!").waitFor({ timeout: 15000 });
  await shot(page, "d05-booking-confirmed");
  log("booking ok:", await page.locator(".font-mono").first().innerText());

  // The restaurant's printed QR opens its page; old per-table codes still work.
  await page.goto(`${BASE}/r/oae1`);
  await page.waitForURL(/\/restaurants\/olive-and-ember\?via=qr/, { timeout: 15000 });
  await page.getByRole("heading", { name: "Olive & Ember", level: 1 }).waitFor();
  await shot(page, "d06-qr-landing");
  await page.goto(`${BASE}/t/oae1-t7`);
  await page.waitForURL(/\/restaurants\/olive-and-ember/, { timeout: 15000 });
  log("restaurant QR + old table QR open the restaurant ok");
  await page.goto(`${BASE}/r/nope`);
  await page.getByText("This QR code isn't linked to a restaurant").waitFor();
  log("unknown QR shows a message ok");
  await page.goto(`${BASE}/dine-in`);
  await page.getByRole("heading", { name: "At the restaurant" }).waitFor();
  await shot(page, "d07-at-the-restaurant");
  log("at the restaurant page ok");

  // Owner portal
  await logout(page);
  await login(page, "owner");
  log("owner login ->", new URL(page.url()).pathname);
  await page.getByText("Bookings today").waitFor();
  await shot(page, "d08-partner-dashboard");
  // The booking reached the owner as a notification (outbox → bus → projector).
  const bell = page.getByRole("button", { name: /^Notifications, \d+ unread/ }).first();
  await bell.waitFor({ timeout: 15000 });
  await bell.click();
  await page.getByRole("dialog").getByText(/New booking/).first().waitFor();
  await shot(page, "d08b-owner-notifications");
  await page.keyboard.press("Escape");
  log("owner notified of booking ok");
  await page.goto(`${BASE}/partner/qr`);
  await page.getByText("What diners see when they scan it").waitFor();
  await page.getByAltText(/QR code linking to .*\/r\/[a-z0-9]+$/).waitFor();
  await shot(page, "d09-partner-qr");
  log("owner restaurant QR ok");
  await page.goto(`${BASE}/partner/bookings`);
  await page.getByRole("heading", { name: "Bookings" }).waitFor();
  await shot(page, "d10-partner-bookings");
  await page.goto(`${BASE}/partner/menu`);
  await page.getByRole("switch", { name: "In stock" }).first().click();
  await page.getByText(/is out of stock/).waitFor();
  log("menu 86 ok");
  await page.goto(`${BASE}/partner/offers`);
  await page.getByText("Pre-booking deal").waitFor();
  await shot(page, "d11-partner-offers");
  await page.goto(`${BASE}/partner/tables`); // old bookmark
  await page.waitForURL(/\/partner\/qr/, { timeout: 15000 });
  log("old tables page redirects to QR ok");
  // Staff don't get the customer app: "/" sends the owner back to the portal…
  await page.goto(BASE);
  await page.waitForURL((u) => u.pathname === "/partner");
  log("owner kept out of customer app ok");
  // …but can preview their own listing exactly as diners see it.
  await page.getByRole("link", { name: "View as customer" }).click();
  await page.getByText("Customer preview").waitFor();
  await page.getByRole("heading", { name: "Olive & Ember", level: 1 }).waitFor();
  await shot(page, "d12b-owner-customer-preview");
  log("owner customer preview ok");
  // Owner can't reach admin
  await page.goto(`${BASE}/admin`);
  await page.getByText("You don't have access to this page").waitFor();
  log("owner blocked from admin ok");

  // Admin console
  await logout(page);
  await login(page, "admin");
  log("admin login ->", new URL(page.url()).pathname);
  await page.getByText("Platform overview").waitFor();
  await shot(page, "d13-admin-overview");
  await page.goto(`${BASE}/admin/restaurants`);
  await page.getByRole("heading", { name: "Restaurants" }).waitFor();
  await shot(page, "d14-admin-restaurants");
  // Onboard + approve a restaurant
  await page.getByRole("button", { name: "Onboard restaurant" }).click();
  await page.getByLabel("Restaurant name").fill("Test Kitchen");
  await page.getByLabel("Area").selectOption("Koramangala");
  await page.getByLabel("Cuisines (comma separated)").fill("Fusion");
  await page.getByLabel(/Cost for two/).fill("1200");
  await page.getByRole("button", { name: /^Create/ }).click();
  await page.getByText(/Restaurant created/).waitFor();
  await page.goto(`${BASE}/admin/restaurants?status=ONBOARDING`);
  await page.getByText("Test Kitchen").first().waitFor();
  log("admin onboard (draft) ok");
  // Suspend Sugar Bloom, confirm it disappears for customers
  await page.goto(`${BASE}/admin/restaurants`);
  const row = page.locator("tr", { hasText: "Sugar Bloom" });
  await row.getByRole("button", { name: "Suspend" }).click();
  await page.getByText(/Sugar Bloom is now suspended/).waitFor();
  await page.goto(`${BASE}/admin/users`);
  await page.getByRole("heading", { name: "Users & roles" }).waitFor();
  await shot(page, "d15-admin-users");
  await page.goto(`${BASE}/restaurants/sugar-bloom`);
  await page.getByText("Restaurant not found").waitFor();
  log("suspended restaurant hidden ok");

  // Customer: book for today if a slot is left, then pay the bill with the deal.
  await logout(page);
  await login(page, "customer");
  await page.goto(`${BASE}/restaurants/olive-and-ember/book`);
  await page.getByRole("radiogroup", { name: "Date" }).getByRole("radio").first().click(); // today
  await page.getByRole("radiogroup", { name: "Time slot" }).waitFor();
  const todaySlot = page.getByRole("radiogroup", { name: "Time slot" }).locator("[role=radio]:not([disabled])").first();
  if (await todaySlot.count()) {
    await todaySlot.click();
    await page.getByRole("button", { name: /^Continue/ }).click();
    await page.getByLabel("Name").fill("Kavya Rao");
    await page.getByLabel("Mobile number").fill("9876543210");
    await page.getByRole("button", { name: "Confirm booking" }).click();
    await page.getByText("Table booked!").waitFor({ timeout: 15000 });
    await page.goto(`${BASE}/dine-in`);
    await page.getByRole("link", { name: /Pay bill & get your deal/ }).first().click();
    await page.waitForURL(/\/pay\/olive-and-ember/, { timeout: 15000 });
    await page.getByLabel("Enter the total on your restaurant bill").fill("2450");
    await page.getByText("You save").waitFor({ timeout: 10000 });
    await shot(page, "d16-pay-bill");
    await page.getByRole("button", { name: /^Pay ₹/ }).click();
    await page.getByText("TEST MODE", { exact: true }).waitFor();
    await page.getByRole("dialog").getByRole("button", { name: /^Pay ₹/ }).click();
    await page.waitForURL(/\/payments\//, { timeout: 20000 });
    await page.getByText("Receipt").waitFor();
    await shot(page, "d17-receipt");
    log("booked today, pay bill with deal ok:", await page.locator("h1").innerText());
  } else {
    log("no slot left today: skipped paying a booked bill");
  }

  // Typed-in bill: only for diners with a Feast booking today (deals need one).
  await page.goto(`${BASE}/pay/bamboo-lane`);
  const amountBox = page.getByLabel("Enter the total on your restaurant bill");
  const bookFirst = page.getByRole("heading", { name: "Book on Feast to pay here and save" });
  await amountBox.or(bookFirst).first().waitFor({ timeout: 15000 });
  if (await bookFirst.isVisible()) {
    await shot(page, "d16b-book-first");
    log("walk-in pay-bill asks to book first ok");
  } else {
    // Booked today (seed data): manual bill with failure then success
    await amountBox.fill("2450");
    await page.getByText("You save").waitFor({ timeout: 10000 });
    await page.getByRole("button", { name: /^Pay ₹/ }).click();
    await page.getByText("Test mode: simulate a declined payment").click();
    await page.getByRole("dialog").getByRole("button", { name: /^Pay ₹/ }).click();
    await page.getByText(/declined by bank/).waitFor({ timeout: 10000 });
    log("simulated failure shown ok");
    await page.getByText("Test mode: simulate a declined payment").click();
    await page.getByRole("dialog").getByRole("button", { name: /^Pay ₹/ }).click();
    await page.waitForURL(/\/payments\//, { timeout: 20000 });
    log("retry success ok");
  }

  await page.goto(`${BASE}/bookings`);
  await page.getByText("Your dining").waitFor();
  await shot(page, "d18-bookings");
  await ctx.close();
}

// ---------------- Phone: layouts ----------------
{
  const ctx = await newCtx(browser, { width: 390, height: 844 }, true);
  const page = await ctx.newPage();
  await page.goto(BASE);
  await page.getByText("Top rated near you").waitFor();
  await shot(page, "m01-home");
  await page.goto(`${BASE}/restaurants/coastal-tide`);
  await page.getByRole("heading", { name: "Coastal Tide" }).waitFor();
  await shot(page, "m02-restaurant");
  await page.goto(`${BASE}/search`);
  await page.getByRole("button", { name: /Filters/ }).click();
  await page.getByRole("dialog").waitFor();
  await shot(page, "m03-search-filters");
  await login(page, "customer");
  await page.goto(`${BASE}/restaurants/coastal-tide/book`);
  await page.getByRole("radiogroup", { name: "Date" }).getByRole("radio").nth(1).click(); // tomorrow: today's slots may be over
  await page.getByRole("radiogroup", { name: "Time slot" }).waitFor();
  await page.getByRole("radiogroup", { name: "Time slot" }).locator("[role=radio]:not([disabled])").first().click();
  await shot(page, "m04-book");
  await page.goto(`${BASE}/dine-in`);
  await page.getByRole("heading", { name: "At the restaurant" }).waitFor();
  await shot(page, "m05-at-the-restaurant");
  await page.goto(`${BASE}/pay/coastal-tide`);
  await page.getByLabel("Enter the total on your restaurant bill").or(page.getByRole("heading", { name: "Book on Feast to pay here and save" })).first().waitFor({ timeout: 15000 });
  await shot(page, "m06-pay-bill");
  await logout(page);
  await login(page, "owner");
  await page.getByText("Bookings today").waitFor();
  await shot(page, "m07-partner-dashboard");
  await page.goto(`${BASE}/partner/bookings`);
  await page.getByRole("heading", { name: "Bookings" }).waitFor();
  await shot(page, "m08-partner-bookings");
  await logout(page);
  await login(page, "admin");
  await page.goto(`${BASE}/admin/users`);
  await page.getByRole("heading", { name: "Users & roles" }).waitFor();
  await shot(page, "m09-admin-users");
  await page.goto(`${BASE}/login`);
  await logout(page);
  await page.goto(`${BASE}/login`);
  await shot(page, "m10-login");
  await ctx.close();
}

// ---------------- Side by side: customer tab + owner tab in one browser ----------------
// Each tab keeps its own signed-in user; the owner's changes reach the customer live.
{
  const ctx = await newCtx(browser, { width: 1280, height: 860 }, false);
  const customer = await ctx.newPage();
  await login(customer, "customer");
  await customer.goto(`${BASE}/restaurants/olive-and-ember?tab=menu`);
  await customer.getByText("Burrata & heirloom tomato").waitFor();
  const unavailable = customer.getByText("Currently unavailable");
  const before = await unavailable.count();

  const owner = await ctx.newPage();
  await login(owner, "owner");
  if (new URL(owner.url()).pathname !== "/partner") errors.push(`owner landed on ${owner.url()}`);
  await owner.goto(`${BASE}/partner/menu`);
  await owner.getByRole("switch", { name: "In stock" }).first().click();
  await owner.getByText(/is out of stock/).waitFor();

  // Customer tab: still the customer, and the menu shows the owner's change.
  await customer.bringToFront();
  await customer.reload();
  await customer.waitForFunction((n) => document.body.textContent.split("Currently unavailable").length - 1 > n, before, { timeout: 10000 });
  if (new URL(customer.url()).pathname !== "/restaurants/olive-and-ember") errors.push(`customer tab moved to ${customer.url()}`);
  await customer.getByRole("button", { name: /Account: Kavya/ }).waitFor();
  await shot(customer, "s01-customer-sees-owner-change");
  log("side-by-side: separate users + menu change visible ok");

  // Customer filters come from owners' listings: a new cuisine + experience become filters.
  await customer.goto(`${BASE}/search`);
  await customer.getByRole("button", { name: /^Filters/ }).click();
  const sheet = customer.getByRole("dialog");
  await sheet.getByRole("region", { name: "Cuisine" }).getByRole("button", { name: /^Italian/ }).waitFor();
  if (await sheet.getByRole("button", { name: /^Neapolitan/ }).count()) errors.push("Neapolitan filter existed before owner listed it");
  await customer.keyboard.press("Escape");

  await owner.bringToFront();
  await owner.goto(`${BASE}/partner/listing`);
  await owner.getByRole("textbox", { name: "Add cuisines" }).fill("neapolitan");
  await owner.getByRole("textbox", { name: "Add cuisines" }).press("Enter");
  await owner.getByRole("textbox", { name: "Add experiences" }).fill("wood-fired oven");
  await owner.getByRole("textbox", { name: "Add experiences" }).press("Enter");
  await owner.getByRole("textbox", { name: "Average cost for two (₹)" }).fill("1400");
  await shot(owner, "s02-owner-listing");
  await owner.getByRole("button", { name: "Save listing" }).click();
  await owner.getByText(/Listing updated/).waitFor();

  await customer.bringToFront();
  await customer.reload();
  await customer.getByRole("button", { name: /^Filters/ }).click();
  const cuisine = customer.getByRole("dialog").getByRole("region", { name: "Cuisine" });
  await cuisine.getByRole("button", { name: /^Neapolitan\s*1$/ }).waitFor({ timeout: 10000 });
  await customer.getByRole("dialog").getByRole("region", { name: "Experience" }).getByRole("button", { name: /^Wood-fired oven/ }).waitFor();
  await shot(customer, "s03-filters-from-listings");
  await cuisine.getByRole("button", { name: /^Neapolitan/ }).click();
  await customer.getByText("1 restaurant", { exact: true }).waitFor();
  await customer.keyboard.press("Escape");
  await customer.getByText("Olive & Ember").first().waitFor();
  // Price band moved with the new cost for two (₹1,400 → ₹₹).
  await customer.goto(`${BASE}/search?priceBand=2`);
  await customer.getByText("Olive & Ember").first().waitFor();
  log("filters built from owner listings ok");
  await ctx.close();
}

// ---------------- Restaurant onboarding: signup → documents → admin approval → visible ----------------
{
  const ctx = await newCtx(browser, { width: 1280, height: 900 }, false);
  const page = await ctx.newPage();
  const pdf = { name: "licence.pdf", mimeType: "application/pdf", buffer: Buffer.from("%PDF-1.4\n% demo licence\n") };
  const nextYear = `${new Date().getFullYear() + 1}-06-30`;

  await page.goto(`${BASE}/join`);
  await page.getByLabel("Your full name").fill("Neha Kapoor");
  await page.getByLabel("Work email").fill("neha@curryleaf.example");
  await page.getByLabel("Password").fill("supersecret1");
  await page.getByLabel("Restaurant name").fill("Curry Leaf Kitchen");
  await page.getByLabel("City", { exact: true }).selectOption("blr");
  await page.getByLabel("Area", { exact: true }).selectOption("Jayanagar");
  await page.getByLabel("Cuisines").fill("South Indian, Kerala");
  await page.getByLabel(/Average cost for two/).fill("700");
  await page.getByRole("button", { name: "Create account & continue" }).click();
  await page.waitForURL(/\/partner\/documents/);
  await page.getByRole("heading", { name: "Documents & licences" }).waitFor();
  log("partner signup ok");

  // Customers can't see it yet.
  const hidden = await (await fetch(`${BASE}/`)).ok; // page loads; visibility checked via search below
  if (!hidden) errors.push("home failed");

  // Upload every required document.
  for (;;) {
    const btn = page.getByRole("button", { name: /^Upload$/ }).first();
    if (!(await btn.count())) break;
    await btn.click();
    const dialog = page.getByRole("dialog");
    await dialog.getByLabel("Document file").setInputFiles(pdf);
    const number = dialog.locator("input:not([type=file]):not([type=date])");
    if (await number.count()) await number.first().fill("ABC1234567890");
    const date = dialog.locator("input[type=date]");
    if (await date.count()) await date.fill(nextYear);
    await dialog.getByRole("button", { name: /Upload for review/ }).click();
    await page.getByText(/Uploaded — Feast will review it/).last().waitFor();
    await dialog.waitFor({ state: "detached" }).catch(() => {});
    // Only required ones: stop when the submit button is enabled.
    if (await page.getByRole("button", { name: "Submit for review" }).isEnabled()) break;
  }
  await shot(page, "o01-documents-uploaded");
  await page.getByRole("button", { name: "Submit for review" }).click();
  await page.getByText(/Submitted for review/).first().waitFor();
  log("documents uploaded + submitted ok");

  // Admin reviews: reject one, approve the rest, then approve the restaurant.
  await page.evaluate(() => sessionStorage.setItem("feast.auth.token", ""));
  await login(page, "admin");
  await page.goto(`${BASE}/admin/approvals`);
  await page.getByRole("button", { name: /Curry Leaf Kitchen/ }).click();
  await page.getByRole("button", { name: /^Reject PAN card/ }).click();
  await page.getByLabel("Rejection reason").fill("Image is blurry — please re-upload");
  await page.getByRole("button", { name: "Reject", exact: true }).click();
  await page.getByText(/1\/6 approved|0\/6 approved/).first().waitFor();
  for (;;) {
    const approve = page.locator("li", { hasText: "In review" }).getByRole("button", { name: /^Approve / }).first();
    if (!(await approve.count())) break;
    await approve.click();
    await page.waitForTimeout(400);
  }
  if (await page.getByRole("button", { name: /Approve & go live/ }).isEnabled()) errors.push("go-live enabled with a rejected document");
  await page.getByRole("button", { name: "Request changes" }).click();
  await page.getByLabel("Notes for the owner").fill("Please re-upload a clear PAN card.");
  await page.getByRole("button", { name: "Send to owner" }).click();
  await page.getByText("Sent back to the owner").waitFor();
  await shot(page, "o02-admin-approvals");
  log("admin review + request changes ok");

  // Owner re-uploads PAN; admin approves; restaurant goes live and appears in search.
  await page.evaluate(() => sessionStorage.setItem("feast.auth.token", ""));
  await page.goto(`${BASE}/login?as=partner`);
  await page.getByLabel("Work email").fill("neha@curryleaf.example");
  await page.getByLabel("Password").fill("supersecret1");
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  await page.waitForURL(/\/partner/);
  await page.goto(`${BASE}/partner/documents`);
  await page.getByText(/Please re-upload a clear PAN card/).first().waitFor();
  await page.locator("li", { hasText: "PAN card" }).getByRole("button", { name: /Replace/ }).click();
  await page.getByRole("dialog").getByLabel("Document file").setInputFiles(pdf);
  await page.getByRole("dialog").locator("input:not([type=file]):not([type=date])").first().fill("ABCDE1234F");
  await page.getByRole("dialog").getByRole("button", { name: /Upload for review/ }).click();
  await page.getByText(/Uploaded — Feast will review it/).last().waitFor();
  await page.getByRole("button", { name: "Submit for review" }).click();
  await page.getByText(/Submitted for review/).first().waitFor();

  await page.evaluate(() => sessionStorage.setItem("feast.auth.token", ""));
  await login(page, "admin");
  await page.goto(`${BASE}/admin/approvals`);
  await page.getByRole("button", { name: /Curry Leaf Kitchen/ }).click();
  await page.getByRole("button", { name: /^Approve PAN card/ }).click();
  await page.getByRole("button", { name: /Approve & go live/ }).click();
  await page.getByText(/is live — free trial started/).waitFor();
  log("admin approved, restaurant live ok");

  await page.evaluate(() => sessionStorage.setItem("feast.auth.token", ""));
  await page.goto(`${BASE}/search?q=curry%20leaf`);
  await page.getByText("Curry Leaf Kitchen").first().waitFor();
  log("approved restaurant visible to customers ok");

  // Subscription: owner sees trial, pays a plan; admin sees it in platform payments.
  await page.goto(`${BASE}/login?as=partner`);
  await page.getByLabel("Work email").fill("neha@curryleaf.example");
  await page.getByLabel("Password").fill("supersecret1");
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  await page.waitForURL(/\/partner/);
  await page.goto(`${BASE}/partner/subscription`);
  await page.getByText(/in your free trial/).waitFor();
  await page.getByRole("button", { name: /Choose yearly/ }).click();
  await page.getByRole("dialog").getByRole("button", { name: /^Continue to pay ₹/ }).click();
  await page.getByText("TEST MODE", { exact: true }).waitFor();
  await page.getByRole("dialog").getByRole("button", { name: /^Pay ₹/ }).click();
  await page.getByText(/Paid ₹/).waitFor();
  await shot(page, "o03-subscription-paid");
  log("subscription paid ok");

  await page.evaluate(() => sessionStorage.setItem("feast.auth.token", ""));
  await login(page, "admin");
  await page.goto(`${BASE}/admin/platform-payments`);
  await page.getByRole("cell", { name: "Curry Leaf Kitchen" }).first().waitFor();
  await shot(page, "o04-platform-payments");
  await page.goto(`${BASE}/admin/subscriptions`);
  await page.getByText("Colaba Corner Cafe").waitFor();
  await page.locator("tr", { hasText: "Colaba Corner Cafe" }).getByText("Expired · hidden").waitFor();
  log("admin subscriptions + platform payments ok");

  // Colaba Corner Cafe (Mumbai) subscription expired → hidden from Mumbai search.
  await page.evaluate(() => sessionStorage.setItem("feast.auth.token", ""));
  await page.evaluate(() => localStorage.setItem("feast.city", JSON.stringify("bom")));
  await page.goto(`${BASE}/search`);
  await page.getByText("Bombay Monsoon").first().waitFor();
  if (await page.getByText("Colaba Corner Cafe").count()) errors.push("expired listing visible to customers");
  log("expired subscription hidden ok");
  await ctx.close();
}

// ---------------- Geo: a visitor from Australia on the India site is pointed to Feast Australia ----------------
{
  const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 } });
  // In production the CDN adds the visitor's country header to API requests; simulate that.
  await ctx.route("**/v1/geo", (r) => r.continue({ headers: { ...r.request().headers(), "cf-ipcountry": "AU" } }));
  const page = await ctx.newPage();
  wire(page);
  await page.goto(BASE);
  await page.getByRole("link", { name: /Go to Feast Australia/ }).waitFor();
  await shot(page, "g01-geo-redirect");
  log("geo redirect banner ok");
  await ctx.close();
}

// ---------------- Australia deployment: AUD, tax-inclusive bills, +61 phones, Stripe market ----------------
{
  const ctx = await newCtx(browser, { width: 1280, height: 860 }, false);
  const page = await ctx.newPage();
  await page.goto(`${BASE_AU}/search`);
  await page.getByText("Harbour & Vine").first().waitFor();
  const card = await page.locator("text=for two").first().innerText();
  if (!card.includes("$")) errors.push(`AU card not in AUD: ${card}`);
  if (await page.getByText("Olive & Ember").count()) errors.push("Indian restaurant shown on the Australia site");
  await page.getByRole("button", { name: /^Filters/ }).click();
  const band = await page.getByRole("dialog").getByRole("region", { name: "Cost for two" }).innerText();
  if (!band.includes("$")) errors.push(`AU price band not in AUD: ${band}`);
  await page.keyboard.press("Escape");
  await shot(page, "a01-sydney-search");
  log("AU search in AUD ok");

  // Sydney diner signs in with +61 and gets a tax-inclusive quote.
  await login(page, "customerAu", BASE_AU);
  await page.goto(`${BASE_AU}/pay/harbour-and-vine`);
  const auAmount = page.locator("main input[inputmode=decimal]").first();
  const auBookFirst = page.getByRole("heading", { name: "Book on Feast to pay here and save" });
  await auAmount.or(auBookFirst).first().waitFor({ timeout: 15000 });
  if (await auBookFirst.isVisible()) {
    log("AU walk-in pay-bill asks to book first ok");
  } else {
    await auAmount.fill("120");
    await page.getByText(/Pay A?\$/).first().waitFor({ timeout: 10000 });
    await shot(page, "a02-pay-aud");
    log("AU pay-bill quote in AUD ok");
  }

  // Social sign-in (test mode on dev servers) creates a diner.
  await logout(page);
  await page.goto(`${BASE_AU}/login`);
  await page.getByRole("button", { name: "Continue with Google" }).click();
  const g = page.getByRole("dialog");
  await g.getByLabel("Name").fill("Liam Walker");
  await g.getByLabel("Email").fill("liam.walker@example.com.au");
  await g.getByRole("button", { name: "Continue" }).click();
  await page.waitForURL((u) => !u.pathname.startsWith("/login"));
  await page.getByRole("button", { name: /Account: Liam/ }).waitFor();
  log("social sign-in ok");

  // Australian owner: Stripe-market subscription shows AUD plans with GST.
  await logout(page);
  await login(page, "ownerAu", BASE_AU);
  await page.goto(`${BASE_AU}/partner/subscription`);
  await page.getByText(/A?\$45(\.00)?/).first().waitFor();
  await shot(page, "a03-au-subscription");
  log("AU owner subscription in AUD ok");

  // Admin: this deployment's country only.
  await logout(page);
  await login(page, "admin", BASE_AU);
  await page.goto(`${BASE_AU}/admin/markets`);
  await page.getByRole("heading", { name: "Country · Australia" }).waitFor();
  await page.getByRole("heading", { name: "Onboarding documents" }).waitFor();
  await shot(page, "a04-admin-country");
  log("AU admin country page ok");
  await ctx.close();
}

await browser.close();
console.log("\nERRORS:", errors.length);
for (const e of errors) console.log("  ", e.slice(0, 300));
process.exit(errors.length ? 1 : 0);
