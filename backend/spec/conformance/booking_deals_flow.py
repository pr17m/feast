"""HTTP conformance test for "book on Feast, pay on Feast" deals against any
Feast backend serving Australia with development seed data:

    python3 booking_deals_flow.py http://localhost:8080     # exit code 1 on any failure

Covers: a walk-in can't pay a typed-in bill on Feast (BOOKING_REQUIRED); a
diner booked at that restaurant today gets the deal when paying on Feast; one
discounted bill per booking (a second payment with the same booking's deal is
refused while the first holds it, and later bills get no deal); a booking at
another restaurant doesn't qualify; the restaurant's QR opens the restaurant
and a walk-in who scans it is still asked to book.

The booked-diner checks need a slot still bookable today in Sydney. Late at
night there may be none: run the server under libfaketime at, say, 18:30 Sydney
and pass the same instant here (see arrival_qr_flow.py):

    TZ=UTC faketime -f '@2026-10-25 07:30:00' feast --country au all
    FEAST_TEST_NOW=2026-10-25T07:30:00Z python3 booking_deals_flow.py http://localhost:8094

Without a slot those checks are reported as SKIP.
"""
import json, os, sys, time, urllib.request, uuid
from datetime import datetime, timezone
from zoneinfo import ZoneInfo

BASE = sys.argv[1].rstrip("/")
ADMIN_PASSWORD = os.environ.get("FEAST_ADMIN_PASSWORD", "feast123")
NOW = datetime.fromisoformat(os.environ["FEAST_TEST_NOW"].replace("Z", "+00:00")) if os.environ.get("FEAST_TEST_NOW") else datetime.now(timezone.utc)


def call(method, path, body=None, token=None):
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(BASE + path, data=data, method=method)
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
        return e.code, (json.loads(b) if b else {})


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


def diner(name):
    phone = f"+614{uuid.uuid4().int % 10**8:08d}"
    call("POST", "/v1/auth/otp", {"phone": phone})
    s, sess = call("POST", "/v1/auth/otp/verify", {"phone": phone, "otp": "123456", "name": name})
    assert s == 200, (s, sess)
    return sess["token"], sess["user"]


def book_today(token, user, r, tz):
    """Book the first slot still open today at `r`; None if there isn't one."""
    today = NOW.astimezone(ZoneInfo(tz)).strftime("%Y-%m-%d")
    s, avail = call("GET", f"/v1/restaurants/{r['id']}/slots?date={today}&guests=2")
    slot = next((sl for sl in (avail.get("slots", []) if s == 200 else []) if sl["available"]), None)
    if not slot:
        return None
    s, res = call("POST", "/v1/bookings", {
        "restaurantId": r["id"], "date": today, "slotId": slot["id"], "guests": 2,
        "contact": {"name": user.get("name") or "Diner", "phone": user["phone"]}, "idempotencyKey": uuid.uuid4().hex,
    }, token=token)
    assert s == 201, (s, res)
    bk = res["booking"]
    if bk["status"] == "PENDING_PAYMENT":  # a cover charge to hold the table
        s, pi = call("POST", "/v1/payments/intents", {"purpose": "BOOKING", "referenceId": bk["id"]}, token=token)
        call("POST", f"/v1/payments/intents/{pi['id']}/confirm", {"method": "CARD"}, token=token)
        s, bk = call("GET", f"/v1/bookings/{bk['id']}", token=token)
    return bk


def quote(token, rid, amount=20000, **extra):
    return call("POST", "/v1/bills/quote", {"restaurantId": rid, "billAmount": amount, "tip": 0, **extra}, token=token)


def intent(token, bill):
    return call("POST", "/v1/payments/intents", {"purpose": "BILL", "referenceId": bill["id"]}, token=token)


def confirm(token, pi, **extra):
    return call("POST", f"/v1/payments/intents/{pi['id']}/confirm", {"method": "CARD", **extra}, token=token)


s, admin = call("POST", "/v1/auth/login", {"email": "admin@feast.demo", "password": ADMIN_PASSWORD})
assert s == 200, admin
at = admin["token"]
s, pho = call("GET", "/v1/restaurants/pho-republic")
s, fitz = call("GET", "/v1/restaurants/fitzroy-fire")
ok(any(o["scope"] == "BILL" for o in pho["offers"]), "pho-republic has a bill offer")

# 1. Walk-in, typed-in bill: refused (book first)
walk_tok, walk = diner("Wally Walkin")
s, e = quote(walk_tok, pho["id"])
ok(s == 409 and e.get("code") == "BOOKING_REQUIRED", f"walk-in can't pay a typed-in bill on Feast ({s} {e.get('code')})")
ok("Book a table" in (e.get("message") or ""), f"…and is told to book: {e.get('message')}")

# 2. A booking elsewhere doesn't qualify
far_tok, far = diner("Fran Fitzroy")
fbk = book_today(far_tok, far, fitz, "Australia/Melbourne")
if fbk is None:
    skip("no fitzroy-fire slot left today (see docstring)")
else:
    s, e = quote(far_tok, pho["id"])
    ok(s == 409 and e.get("code") == "BOOKING_REQUIRED", "a booking at another restaurant doesn't qualify")
    s, e = quote(far_tok, pho["id"], bookingId=fbk["id"])
    ok(s == 409 and e.get("code") == "BOOKING_REQUIRED", "…not even when named explicitly")

# 3. Booked diner: deal on the first bill only
bea_tok, bea = diner("Bea Booked")
bk = book_today(bea_tok, bea, pho, "Australia/Sydney")
if bk is None:
    skip("no pho-republic slot left today (see docstring)")
else:
    s, a = quote(bea_tok, pho["id"], 20000)
    ok(s == 200 and a["savings"] > 0 and a.get("bookingId") == bk["id"], f"booked diner gets the deal, found automatically ({a.get('savings')})")
    s, b = quote(bea_tok, pho["id"], 30000, bookingId=bk["id"])
    ok(s == 200 and b["savings"] > 0, "…and when the booking is named")
    s, other = quote(walk_tok, pho["id"], 20000, bookingId=bk["id"])
    ok(s == 409 and other.get("code") == "BOOKING_REQUIRED", "someone else's booking doesn't qualify")
    # Two bills for the same booking: the first payment holds the deal.
    s, pa = intent(bea_tok, a)
    ok(s == 201, "1st bill's payment holds the booking's deal")
    s, e = intent(bea_tok, b)
    ok(s == 409 and e.get("code") == "BOOKING_DEAL_USED", f"2nd discounted bill for the same booking refused ({s} {e.get('code')})")
    s, paid = confirm(bea_tok, pa)
    ok(s == 200 and paid["status"] == "SUCCEEDED", "1st bill paid with the deal")
    s, c = quote(bea_tok, pho["id"], 20000)
    ok(s == 200 and c["savings"] == 0 and c["payable"] == c["billAmount"], "later bills for the booking get no deal")
    ok("already used" in (c.get("dealNotice") or ""), f"…and say why: {c.get('dealNotice')}")
    s, pc = intent(bea_tok, c)
    s, paid = confirm(bea_tok, pc)
    ok(paid.get("status") == "SUCCEEDED", "full-price bill still payable on Feast")

# 4. The restaurant's QR opens its page; a walk-in scanning it still can't pay on Feast
s, qr = call("GET", f"/v1/partner/restaurants/{pho['id']}/qr", token=at)
s, r = call("GET", f"/v1/qr/{qr['code']}")
ok(s == 200 and r["restaurant"]["id"] == pho["id"], "the restaurant QR opens the restaurant")
tw_tok, tw = diner("Quinn Scanner")
s, e = quote(tw_tok, r["restaurant"]["id"], 8000)
ok(s == 409 and e.get("code") == "BOOKING_REQUIRED", f"a walk-in who scans it is asked to book ({s} {e.get('code')})")

print("FAILURES:", fails, "SKIPPED:", skips)
sys.exit(1 if fails else 0)
