"""HTTP conformance test for what the App Store and Google Play require of the
server, against any Feast backend serving Australia with development seed data
(admin@feast.demo, owner.au@feast.demo who owns harbour-and-vine):

    python3 store_flow.py http://localhost:8080     # exit code 1 on any failure

Covers: phone push tokens (register/unregister, who may register which app,
validation, the per-user on/off switch); in-app account deletion for diners
(fresh code + the word DELETE, upcoming bookings cancelled, personal details
gone from the restaurant's booking list and the admin user list, old session
refused, receipts kept without the name, the phone number can sign up again);
owners (password, co-owned restaurants stay live, a sole owner with a counter
payment waiting is refused until it's decided, then their restaurant is
paused); admins can't delete themselves.

Optional, reported as SKIP when not configured:
  FEAST_REVIEW_NUMBER / FEAST_REVIEW_CODE  the server's auth.otp.review_numbers /
      review_code: the reviewer signs in with the review code, not a texted one,
      and the sign-in is audited.
  FEAST_WEB_URL  a running web app with FEAST_IOS_APP_IDS / FEAST_ANDROID_APPS
      set: checks /.well-known/apple-app-site-association and assetlinks.json.

Counter payments need slots still bookable today in Sydney (run the server
under libfaketime as in booking_deals_flow.py); without them those checks SKIP.
Run this suite last: it pauses and re-opens a demo restaurant.
"""
import json, os, sys, urllib.request, uuid
from datetime import datetime, timedelta, timezone
from zoneinfo import ZoneInfo

BASE = sys.argv[1].rstrip("/")
PASSWORD = os.environ.get("FEAST_ADMIN_PASSWORD", "feast123")
OTP = os.environ.get("FEAST_FIXED_OTP", "123456")
NOW = datetime.fromisoformat(os.environ["FEAST_TEST_NOW"].replace("Z", "+00:00")) if os.environ.get("FEAST_TEST_NOW") else datetime.now(timezone.utc)
SYD = ZoneInfo("Australia/Sydney")
TODAY = NOW.astimezone(SYD).strftime("%Y-%m-%d")
TOMORROW = (NOW.astimezone(SYD) + timedelta(days=1)).strftime("%Y-%m-%d")
FORMER = "Former Feast diner"


def call(method, path, body=None, token=None, base=None):
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request((base or BASE) + path, data=data, method=method)
    if data is not None:
        req.add_header("content-type", "application/json")
    if token:
        req.add_header("authorization", "Bearer " + token)
    try:
        with urllib.request.urlopen(req) as r:
            b = r.read()
            return r.status, (json.loads(b) if b else None)
    except urllib.error.HTTPError as e:
        b = e.read()
        try:
            return e.code, (json.loads(b) if b else {})
        except ValueError:
            return e.code, {}


fails = skips = 0


def ok(cond, msg):
    global fails
    print(("PASS " if cond else "FAIL ") + msg)
    if not cond:
        fails += 1


def skip(msg):
    global skips
    skips += 1
    print("SKIP " + msg)


def login(email):
    s, r = call("POST", "/v1/auth/login", {"email": email, "password": PASSWORD})
    assert s == 200, (email, s, r)
    return r["token"], r["user"]


def diner(name, phone=None):
    phone = phone or f"+614{uuid.uuid4().int % 10**8:08d}"
    call("POST", "/v1/auth/otp", {"phone": phone})
    s, sess = call("POST", "/v1/auth/otp/verify", {"phone": phone, "otp": OTP, "name": name})
    assert s == 200, (s, sess)
    return sess["token"], sess["user"]


def book(token, user, r, date):
    s, avail = call("GET", f"/v1/restaurants/{r['id']}/slots?date={date}&guests=2")
    slot = next((sl for sl in (avail.get("slots", []) if s == 200 else []) if sl["available"]), None)
    if not slot:
        return None
    s, res = call("POST", "/v1/bookings", {
        "restaurantId": r["id"], "date": date, "slotId": slot["id"], "guests": 2,
        "contact": {"name": user.get("name") or "Diner", "phone": user["phone"]}, "idempotencyKey": uuid.uuid4().hex,
    }, token=token)
    assert s == 201, (s, res)
    bk = res["booking"]
    if bk["status"] == "PENDING_PAYMENT":
        s, pi = call("POST", "/v1/payments/intents", {"purpose": "BOOKING", "referenceId": bk["id"]}, token=token)
        call("POST", f"/v1/payments/intents/{pi['id']}/confirm", {"method": "CARD"}, token=token)
        s, bk = call("GET", f"/v1/bookings/{bk['id']}", token=token)
    return bk


def delete_me(token, **body):
    return call("DELETE", "/v1/me", body, token=token)


def ptoken():
    return "fcm-" + uuid.uuid4().hex + uuid.uuid4().hex


at, admin = login("admin@feast.demo")
ot, owner = login("owner.au@feast.demo")
s, hv = call("GET", "/v1/restaurants/harbour-and-vine")
rid = hv["id"]

# 1. Push tokens.
dt, du = diner("Pia Push")
tok = ptoken()
s, _ = call("POST", "/v1/me/push-tokens", {"platform": "ios", "token": tok, "app": "diner"})
ok(s == 401, f"registering a push token needs a session ({s})")
s, reg = call("POST", "/v1/me/push-tokens", {"platform": "ios", "token": tok, "app": "diner"}, token=dt)
ok(s == 201 and reg.get("token") == tok and reg.get("platform") == "ios" and reg.get("app") == "diner", f"diner registers an iPhone token ({s})")
s, again = call("POST", "/v1/me/push-tokens", {"platform": "ios", "token": tok, "app": "diner"}, token=dt)
ok(s == 201, f"registering the same token again is fine ({s})")
s, e = call("POST", "/v1/me/push-tokens", {"platform": "ios", "token": "short", "app": "diner"}, token=dt)
ok(s == 400 and e.get("code") == "INVALID_PUSH_TOKEN", f"a malformed token is refused ({s} {e.get('code')})")
s, e = call("POST", "/v1/me/push-tokens", {"platform": "windows", "token": ptoken(), "app": "diner"}, token=dt)
ok(s in (400, 422), f"an unknown platform is refused ({s})")
s, e = call("POST", "/v1/me/push-tokens", {"platform": "android", "token": ptoken(), "app": "partner"}, token=dt)
ok(s == 403, f"a diner can't register the partner app ({s})")
otok = ptoken()
s, _ = call("POST", "/v1/me/push-tokens", {"platform": "android", "token": otok, "app": "partner"}, token=ot)
ok(s == 201, f"an owner registers the partner app on Android ({s})")
s, _ = call("DELETE", f"/v1/me/push-tokens/{otok}", token=ot)
ok(s == 204, f"owner signs out on that phone: token removed ({s})")
s, ps = call("GET", "/v1/me/push-settings", token=dt)
ok(s == 200 and ps.get("enabled") is True, "phone notifications are on by default")
s, ps = call("PUT", "/v1/me/push-settings", {"enabled": False}, token=dt)
ok(s == 200 and ps.get("enabled") is False, "diner turns phone notifications off")
s, ps = call("GET", "/v1/me/push-settings", token=dt)
ok(s == 200 and ps.get("enabled") is False, "…and it sticks")
call("PUT", "/v1/me/push-settings", {"enabled": True}, token=dt)

# 2. Diner deletes their account.
s, e = delete_me(at, confirm="DELETE", password=PASSWORD)
ok(s == 403 and e.get("code") == "ADMIN_CANNOT_SELF_DELETE", f"admins can't delete their own account ({s} {e.get('code')})")

future = book(dt, du, hv, TOMORROW)
ok(future is not None and future["status"] == "CONFIRMED", f"diner books harbour-and-vine for tomorrow ({future and future['status']})")
receipt = None
today_bk = book(dt, du, hv, TODAY)
if today_bk is None:
    skip("no harbour-and-vine slot left today: receipt-kept check")
else:
    s, q = call("POST", "/v1/bills/quote", {"restaurantId": rid, "billAmount": 6400, "tip": 0, "bookingId": today_bk["id"]}, token=dt)
    s, pi = call("POST", "/v1/payments/intents", {"purpose": "BILL", "referenceId": q["id"]}, token=dt)
    s, paid = call("POST", f"/v1/payments/intents/{pi['id']}/confirm", {"method": "CARD"}, token=dt)
    s, mine = call("GET", "/v1/me/bill-payments", token=dt)
    items = mine.get("items", mine) if isinstance(mine, dict) else mine
    receipt = next((p for p in (items or []) if p.get("restaurant", {}).get("id") == rid), None)
    ok(receipt is not None, "diner paid a bill on Feast (receipt)")

s, e = delete_me(dt)
ok(s == 400 and e.get("code") == "CONFIRM_REQUIRED", f"the word DELETE is required ({s} {e.get('code')})")
s, e = delete_me(dt, confirm="DELETE")
ok(s == 400 and e.get("code") == "OTP_REQUIRED", f"a fresh code is required ({s} {e.get('code')})")
s, e = delete_me(dt, confirm="DELETE", otp="000000" if OTP != "000000" else "111111")
ok(s == 400 and e.get("code") == "BAD_OTP", f"a wrong code is refused, without signing the diner out ({s} {e.get('code')})")
s, me = call("GET", "/v1/me", token=dt)
ok(s == 200 and me["id"] == du["id"], "account still there after the failed attempts")
call("POST", "/v1/auth/otp", {"phone": du["phone"]})
s, res = delete_me(dt, confirm="delete", otp=OTP)
ok(s == 200 and res.get("deleted") is True and res.get("cancelledBookings", 0) >= 1, f"diner deletes their account ({s} {res})")
s, _ = call("GET", "/v1/me", token=dt)
ok(s == 401, f"the old session is refused ({s})")
s, _ = call("POST", "/v1/me/push-tokens", {"platform": "ios", "token": ptoken(), "app": "diner"}, token=dt)
ok(s == 401, f"…including for push tokens ({s})")
if future:
    s, day = call("GET", f"/v1/partner/restaurants/{rid}/bookings?date={TOMORROW}", token=ot)
    b = next((x for x in (day or []) if x["id"] == future["id"]), None)
    ok(b is not None and b["status"] == "CANCELLED", f"tomorrow's booking is cancelled for the restaurant ({b and b['status']})")
    ok(b is not None and b["contact"]["name"] == FORMER and not b["contact"].get("phone"), f"…and shows no name or phone ({b and b['contact']})")
    s, log = call("GET", f"/v1/admin/audit?entityType=booking&entityId={future['id']}", token=at)
    ok(s == 200 and any(r["action"] == "booking.cancelled" for r in log["items"]), "the cancellation is in the audit trail")
s, users = call("GET", "/v1/admin/users", token=at)
gone = next((u for u in users if u["id"] == du["id"]), None)
ok(gone is not None and gone["status"] == "DELETED" and gone["name"] == FORMER and not gone.get("phone") and not gone.get("email"), f"admin sees an anonymised, deleted user ({gone})")
s, log = call("GET", f"/v1/admin/audit?entityType=user&entityId={du['id']}", token=at)
ok(s == 200 and any(r["action"] == "user.account_deleted" and r["actor"].get("userId") == du["id"] for r in log["items"]), "deletion audited, by the diner")
if receipt:
    s, pays = call("GET", f"/v1/partner/restaurants/{rid}/payments", token=ot)
    ok(s == 200 and any(p["id"] == receipt["id"] and p["userId"] == du["id"] for p in pays), "the restaurant keeps the receipt (without the diner's details)")
s, e = call("PATCH", f"/v1/admin/users/{du['id']}", {"status": "ACTIVE"}, token=at)
ok(s == 409 and e.get("code") == "ACCOUNT_DELETED", f"a deleted account can't be re-activated ({s} {e.get('code')})")
nt, nu = diner("Pia Again", du["phone"])
ok(nu["id"] != du["id"], "the phone number can sign up again, as a new account")

# 3. Owners.
s, co = call("POST", "/v1/admin/users", {"name": "Cora Coowner", "email": f"cora.{uuid.uuid4().hex[:8]}@feast.test", "role": "OWNER", "restaurantIds": [rid]}, token=at)
ok(s in (200, 201), f"admin adds a co-owner of harbour-and-vine ({s})")
ct, cu = login(co["email"])
s, e = delete_me(ct, confirm="DELETE", password="not-my-password")
ok(s == 400 and e.get("code") == "BAD_PASSWORD", f"owners re-enter their password ({s} {e.get('code')})")
s, res = delete_me(ct, confirm="DELETE", password=PASSWORD)
ok(s == 200 and res.get("pausedRestaurants") == [], f"co-owner deletes their account; the restaurant stays live ({s} {res})")
s, r = call("GET", "/v1/restaurants/harbour-and-vine")
ok(s == 200, "harbour-and-vine is still listed")
s, _ = call("POST", "/v1/auth/login", {"email": co["email"], "password": PASSWORD})
ok(s == 401, f"the deleted owner can't sign in ({s})")

# A sole owner: a live AU restaurant nobody owns yet.
s, rests = call("GET", "/v1/admin/restaurants", token=at)
s, users = call("GET", "/v1/admin/users", token=at)
owned = {x for u in users if u["role"] == "OWNER" and u.get("status") != "DELETED" for x in u.get("restaurantIds", [])}
candidates = [r for r in rests if r["status"] == "LIVE" and r["id"] not in owned and r["id"] != rid]


def open_today(r):
    s, avail = call("GET", f"/v1/restaurants/{r['id']}/slots?date={TODAY}&guests=2")
    return s == 200 and any(sl["available"] for sl in avail.get("slots", []))


# Prefer one still taking bookings today, so a diner can be waiting on a counter payment.
solo = next((r for r in candidates if open_today(r)), candidates[0] if candidates else None)
if solo is None:
    skip("no unowned live restaurant to test a sole owner's deletion")
else:
    s, so = call("POST", "/v1/admin/users", {"name": "Sol Soleowner", "email": f"sol.{uuid.uuid4().hex[:8]}@feast.test", "role": "OWNER", "restaurantIds": [solo["id"]]}, token=at)
    st, su = login(so["email"])
    pt, pu = diner("Cash Counter")
    pbk = book(pt, pu, solo, TODAY)
    op = None
    if pbk is None:
        skip(f"no {solo['slug']} slot left today: open counter payment blocks deletion")
    else:
        s, q = call("POST", "/v1/bills/quote", {"restaurantId": solo["id"], "billAmount": 5000, "tip": 0, "bookingId": pbk["id"]}, token=pt)
        s, op = call("POST", f"/v1/bills/{q['id']}/offline-payment", {"method": "CASH"}, token=pt)
        ok(s == 201, f"a diner chooses to pay at {solo['slug']}'s counter ({s})")
        s, e = delete_me(st, confirm="DELETE", password=PASSWORD)
        ok(s == 409 and e.get("code") == "OWNER_HAS_OPEN_ITEMS", f"the sole owner must decide it first ({s} {e.get('code')})")
        call("POST", "/v1/auth/otp", {"phone": pu["phone"]})
        s, e = delete_me(pt, confirm="DELETE", otp=OTP)
        ok(s == 409 and e.get("code") == "COUNTER_PAYMENT_PENDING", f"so must the diner who's waiting on it ({s} {e.get('code')})")
        s, _ = call("POST", f"/v1/partner/offline-payments/{op['id']}/reject", {"reason": "Closing the account, please pay by card"}, token=st)
        ok(s == 200, "owner rejects it with a reason")
    s, res = delete_me(st, confirm="DELETE", password=PASSWORD)
    ok(s == 200 and res.get("pausedRestaurants") == [solo["name"]], f"sole owner deletes their account; their restaurant is paused ({s} {res})")
    s, rests = call("GET", "/v1/admin/restaurants", token=at)
    now = next((r for r in rests if r["id"] == solo["id"]), {})
    ok(now.get("status") == "PAUSED", f"admin sees {solo['slug']} paused ({now.get('status')})")
    s, log = call("GET", f"/v1/admin/audit?entityType=user&entityId={su['id']}", token=at)
    ok(s == 200 and any(r["action"] == "user.account_deleted" for r in log["items"]), "owner deletion audited")
    s, _ = call("POST", f"/v1/admin/restaurants/{solo['id']}/status", {"status": "LIVE"}, token=at)
    ok(s == 200, f"admin re-opens it ({s})")

# 4. App-store reviewer sign-in.
rn, rc = os.environ.get("FEAST_REVIEW_NUMBER"), os.environ.get("FEAST_REVIEW_CODE")
if not (rn and rc):
    skip("FEAST_REVIEW_NUMBER/FEAST_REVIEW_CODE not set: reviewer sign-in")
else:
    s, _ = call("POST", "/v1/auth/otp", {"phone": rn})
    ok(s in (200, 202, 204), f"reviewer asks for a code (none is texted) ({s})")
    s, e = call("POST", "/v1/auth/otp/verify", {"phone": rn, "otp": OTP if OTP != rc else "000001", "name": "App Review"})
    ok(s == 401, f"the usual test code doesn't work for a review number ({s})")
    s, sess = call("POST", "/v1/auth/otp/verify", {"phone": rn, "otp": rc, "name": "App Review"})
    ok(s == 200 and sess["user"]["role"] == "CUSTOMER", f"reviewer signs in with the review code ({s})")
    s, log = call("GET", "/v1/admin/audit?entityType=auth", token=at)
    rec = next((r for r in (log or {}).get("items", []) if r["action"] == "auth.review_sign_in"), None)
    ok(rec is not None and rn not in json.dumps(rec), "reviewer sign-in is audited, without the full number")

# 5. App links on the web app.
web = os.environ.get("FEAST_WEB_URL", "").rstrip("/")
if not web:
    skip("FEAST_WEB_URL not set: /.well-known app links")
else:
    s, aasa = call("GET", "/.well-known/apple-app-site-association", base=web)
    if s == 404:
        skip("web app has no FEAST_IOS_APP_IDS")
    else:
        comps = aasa["applinks"]["details"][0]["components"]
        ok(s == 200 and aasa["applinks"]["details"][0]["appIDs"], "apple-app-site-association lists the iOS apps")
        ok({"/v1/*", "/admin/*"} <= {c["/"] for c in comps if c.get("exclude")}, "…and keeps /v1 and /admin on the web")
    s, links = call("GET", "/.well-known/assetlinks.json", base=web)
    if s == 404:
        skip("web app has no FEAST_ANDROID_APPS")
    else:
        ok(s == 200 and links and links[0]["target"]["namespace"] == "android_app", "assetlinks.json lists the Android apps")

print("FAILURES:", fails, "SKIPPED:", skips)
sys.exit(1 if fails else 0)
