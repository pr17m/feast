"""HTTP conformance test for counter (offline) payments and the audit trail,
against any Feast backend serving Australia with development seed data
(admin@feast.demo, owner.au@feast.demo who owns harbour-and-vine):

    python3 offline_audit_flow.py http://localhost:8080     # exit code 1 on any failure

Covers: a booked diner pays their bill in cash at the counter with their Feast
deal and the owner confirms it (receipt marked offline, insights/admin GMV keep
it apart); the owner rejects one (reason required) and the diner can pay again;
the booking's second bill doesn't get the deal; an online payment can't start
while a counter payment waits; another restaurant's owner can't decide; a
walk-in without a booking can't pay on Feast; every step is in the audit trail
with the right actor; diners can't read other people's history.

Counter payments are for Feast bookings, so these checks need slots still
bookable today in Sydney: run the server under libfaketime at ~18:30 Sydney and
pass the same instant (see booking_deals_flow.py). Without them the booked-diner
checks are reported as SKIP.
"""
import json, os, sys, urllib.request, uuid
from datetime import datetime, timezone
from zoneinfo import ZoneInfo

BASE = sys.argv[1].rstrip("/")
PASSWORD = os.environ.get("FEAST_ADMIN_PASSWORD", "feast123")
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


def login(email):
    s, r = call("POST", "/v1/auth/login", {"email": email, "password": PASSWORD})
    assert s == 200, (email, s, r)
    return r["token"], r["user"]


def diner(name):
    phone = f"+614{uuid.uuid4().int % 10**8:08d}"
    call("POST", "/v1/auth/otp", {"phone": phone})
    s, sess = call("POST", "/v1/auth/otp/verify", {"phone": phone, "otp": "123456", "name": name})
    assert s == 200, (s, sess)
    return sess["token"], sess["user"]


def audit(token, path):
    s, page = call("GET", path, token=token)
    assert s == 200, (path, s, page)
    return page["items"]


def book_today(token, user, r, tz):
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
    if bk["status"] == "PENDING_PAYMENT":
        s, pi = call("POST", "/v1/payments/intents", {"purpose": "BOOKING", "referenceId": bk["id"]}, token=token)
        call("POST", f"/v1/payments/intents/{pi['id']}/confirm", {"method": "CARD"}, token=token)
        s, bk = call("GET", f"/v1/bookings/{bk['id']}", token=token)
    return bk


def booked_bill(name, amount):
    """A new diner books today and types in `amount` from the restaurant's bill."""
    tok, user = diner(name)
    bk = book_today(tok, user, hv, "Australia/Sydney")
    if bk is None:
        return None
    s, q = call("POST", "/v1/bills/quote", {"restaurantId": rid, "billAmount": amount, "tip": 0, "bookingId": bk["id"]}, token=tok)
    assert s == 200, (s, q)
    return tok, user, bk, q


at, admin = login("admin@feast.demo")
ot, owner = login("owner.au@feast.demo")
s, hv = call("GET", "/v1/restaurants/harbour-and-vine")
rid = hv["id"]
ok(rid in owner.get("restaurantIds", []), "owner.au owns harbour-and-vine")

# Another restaurant's owner (created by an admin) for the permission checks.
s, other = call("POST", "/v1/admin/users", {"name": "Otto Other", "email": f"otto.{uuid.uuid4().hex[:8]}@feast.test", "role": "OWNER", "restaurantIds": ["r_pho_republic"]}, token=at)
ok(s == 201 or s == 200, f"admin creates another owner ({s})")
xt, _ = login(other["email"])

# 0. A walk-in without a Feast booking can't pay on Feast (no deal at the till either).
wt, walk = diner("Wren Walkin")
s, e = call("POST", "/v1/bills/quote", {"restaurantId": rid, "billAmount": 8000, "tip": 0}, token=wt)
ok(s == 409 and e.get("code") == "BOOKING_REQUIRED", f"walk-in is asked to book first ({s} {e.get('code')})")

# 1. A booked diner pays the bill in cash at the counter; the owner confirms it.
first = booked_bill("Penny Paysatcounter", 12000)
if first is None:
    skip("no harbour-and-vine slot left today (see docstring): counter payment checks")
else:
    pt, payer, pbk, q = first
    ok(q["savings"] > 0 and q.get("bookingId") == pbk["id"], f"booked diner's bill has the Feast deal ({q.get('savings')})")
    s, op = call("POST", f"/v1/bills/{q['id']}/offline-payment", {"method": "CASH"}, token=pt)
    ok(s == 201 and op["status"] == "PENDING_APPROVAL" and op["amount"] == q["payable"], f"diner chooses to pay in cash at the counter ({s} {op.get('status')})")
    s, again = call("POST", f"/v1/bills/{q['id']}/offline-payment", {"method": "CASH"}, token=pt)
    ok(s in (200, 201) and again["id"] == op["id"], "tapping twice keeps the same counter payment")
    s, e = call("POST", "/v1/payments/intents", {"purpose": "BILL", "referenceId": q["id"]}, token=pt)
    ok(s == 409 and e.get("code") == "OFFLINE_PENDING", f"no card payment while the counter payment waits ({s} {e.get('code')})")
    s, st = call("GET", f"/v1/bills/{q['id']}/offline-payment", token=pt)
    ok(s == 200 and st["status"] == "PENDING_APPROVAL", "diner sees it waiting")
    s, queue = call("GET", f"/v1/partner/restaurants/{rid}/offline-payments?status=PENDING_APPROVAL", token=ot)
    ok(s == 200 and any(p["id"] == op["id"] for p in queue), "owner sees it in the approval queue")
    s, e = call("POST", f"/v1/partner/offline-payments/{op['id']}/approve", {}, token=xt)
    ok(s in (403, 404), f"another restaurant's owner can't approve it ({s})")
    s, e = call("POST", f"/v1/partner/offline-payments/{op['id']}/approve", {"amountReceived": op["amount"] - 100}, token=ot)
    ok(s == 409 and e.get("code") == "WRONG_AMOUNT", "approval amount must match the bill")
    s, approved = call("POST", f"/v1/partner/offline-payments/{op['id']}/approve", {"amountReceived": op["amount"]}, token=ot)
    ok(s == 200 and approved["status"] == "APPROVED" and approved.get("billPaymentId"), f"owner confirms the cash ({s})")
    s, e = call("POST", f"/v1/partner/offline-payments/{op['id']}/reject", {"reason": "changed my mind"}, token=ot)
    ok(s == 409 and e.get("code") == "ALREADY_DECIDED", "a decision is final")
    s, receipt = call("GET", f"/v1/bill-payments/{approved['billPaymentId']}", token=pt)
    ok(s == 200 and receipt["payment"]["settlement"] == "OFFLINE" and receipt["payment"]["method"] == "CASH", "receipt is marked paid at the counter")
    s, e = call("POST", "/v1/payments/intents", {"purpose": "BILL", "referenceId": q["id"]}, token=pt)
    ok(s == 409 and e.get("code") == "ALREADY_PAID", "the bill can't be paid again")
    s, ins = call("GET", f"/v1/partner/restaurants/{rid}/insights?days=7", token=ot)
    ok(s == 200 and ins.get("offlineRevenue", 0) >= op["amount"] and ins.get("offlineBills", 0) >= 1, "owner insights count it as settled at the counter")
    s, ains = call("GET", "/v1/admin/insights?days=7", token=at)
    aud = next((m for m in ains["money"] if m["currency"] == "AUD"), {})
    ok(aud.get("offlineSettled", 0) >= op["amount"], "admin insights keep it out of GMV (settled offline)")

    # The trail for that payment, with who did each step.
    hist = audit(pt, f"/v1/bill-payments/{approved['billPaymentId']}/history")
    acts = {h["action"]: h for h in hist}
    ok({"payment.offline_submitted", "payment.offline_approved", "payment.succeeded"} <= set(acts), f"payment history has submit → approve → paid ({sorted(acts)})")
    ok(acts.get("payment.offline_submitted", {}).get("actor", {}).get("userId") == payer["id"], "…submitted by the diner")
    ok(acts.get("payment.offline_approved", {}).get("actor", {}).get("userId") == owner["id"], "…approved by the owner")
    ok(acts.get("payment.offline_approved", {}).get("actor", {}).get("requestId"), "…with the request id")
    s, e = call("GET", f"/v1/bill-payments/{approved['billPaymentId']}/history", token=diner("Nosy Nell")[0])
    ok(s == 404, "another diner can't read that history")
    log = audit(ot, f"/v1/partner/restaurants/{rid}/audit?entityId={op['id']}")
    ok(len(log) >= 2 and all(r["restaurantId"] == rid for r in log), "owner's activity log has it")
    s, e = call("GET", f"/v1/partner/restaurants/{rid}/audit", token=xt)
    ok(s in (403, 404), f"another owner can't read this restaurant's log ({s})")
    s, e = call("GET", "/v1/admin/audit", token=ot)
    ok(s == 403, f"owners can't use the admin audit explorer ({s})")
    bks = audit(at, f"/v1/admin/audit?entityType=booking&entityId={pbk['id']}")
    ok(any(r["action"] in ("booking.created", "booking.confirmed") and r["actor"].get("userId") == payer["id"] for r in bks), "booking made, by the diner")

# 2. Reject: reason required; the diner can pay again (here online).
second = booked_bill("Rex Reject", 9000)
if second is None:
    skip("no harbour-and-vine slot left today: rejection checks")
else:
    rt, rd, rbk, q2 = second
    s, op2 = call("POST", f"/v1/bills/{q2['id']}/offline-payment", {"method": "CARD_AT_COUNTER"}, token=rt)
    ok(s == 201, "diner chooses card at the counter")
    s, e = call("POST", f"/v1/partner/offline-payments/{op2['id']}/reject", {"reason": "no"}, token=ot)
    ok(s == 409 and e.get("code") == "REASON_REQUIRED", "rejecting needs a reason")
    s, rej = call("POST", f"/v1/partner/offline-payments/{op2['id']}/reject", {"reason": "Card was declined at our terminal"}, token=ot)
    ok(s == 200 and rej["status"] == "REJECTED", "owner rejects it with a reason")
    s, pi = call("POST", "/v1/payments/intents", {"purpose": "BILL", "referenceId": q2["id"]}, token=rt)
    ok(s == 201, f"diner can pay again after a rejection ({s})")
    log = audit(at, f"/v1/admin/audit?entityId={op2['id']}")
    rej_rec = next((r for r in log if r["action"] == "payment.offline_rejected"), None)
    ok(rej_rec and "declined" in (rej_rec.get("note") or "") and rej_rec["fromStatus"] == "PENDING_APPROVAL" and rej_rec["toStatus"] == "REJECTED", "rejection audited with reason and status change")

# 3. Booked diner: the deal applies at the counter, once per booking.
bt, bd = diner("Bo Booked")
bk = book_today(bt, bd, hv, "Australia/Sydney")
if bk is None:
    skip("no harbour-and-vine slot left today (see docstring)")
else:
    s, qb = call("POST", "/v1/bills/quote", {"restaurantId": rid, "billAmount": 20000, "tip": 0, "bookingId": bk["id"]}, token=bt)
    ok(s == 200 and qb["savings"] > 0, f"booked diner's bill has the Feast deal ({qb.get('savings')})")
    s, opb = call("POST", f"/v1/bills/{qb['id']}/offline-payment", {"method": "CASH"}, token=bt)
    ok(s == 201 and opb["savings"] == qb["savings"] and opb["amount"] == qb["payable"], "counter payment keeps the deal")
    s, qb2 = call("POST", "/v1/bills/quote", {"restaurantId": rid, "billAmount": 15000, "tip": 0, "bookingId": bk["id"]}, token=bt)
    s, e = call("POST", f"/v1/bills/{qb2['id']}/offline-payment", {"method": "CASH"}, token=bt) if qb2.get("savings", 0) > 0 else (409, {"code": "NO_DEAL"})
    ok(s == 409, f"a second discounted bill for the same booking is refused while the first waits ({e.get('code')})")
    s, apb = call("POST", f"/v1/partner/offline-payments/{opb['id']}/approve", {}, token=ot)
    ok(s == 200 and apb["status"] == "APPROVED", "owner confirms the discounted counter payment")
    s, qb3 = call("POST", "/v1/bills/quote", {"restaurantId": rid, "billAmount": 15000, "tip": 0, "bookingId": bk["id"]}, token=bt)
    ok(s == 200 and qb3["savings"] == 0, "the booking's deal is used: later bills get none")
    hist = audit(bt, f"/v1/bill-payments/{apb['billPaymentId']}/history")
    acts = [h["action"] for h in hist]
    ok("deal.claimed" in acts and "deal.used" in acts, f"deal held then used, in the payment history ({acts})")
    bh = audit(bt, f"/v1/bookings/{bk['id']}/history")
    ok(any(h["action"] in ("booking.created", "booking.confirmed") for h in bh), "booking history has its creation")

# 4. Cancellations are audited with who cancelled.
ct, cd = diner("Cass Cancel")
cb = book_today(ct, cd, hv, "Australia/Sydney")
if cb is None:
    skip("no slot to book and cancel")
else:
    s, _ = call("POST", f"/v1/bookings/{cb['id']}/cancel", {}, token=ct)
    bh = audit(ct, f"/v1/bookings/{cb['id']}/history")
    canc = next((h for h in bh if h["action"] == "booking.cancelled"), None)
    ok(canc and canc["actor"].get("userId") == cd["id"] and "diner" in (canc.get("note") or ""), "cancellation audited: by the diner")
    s, e = call("GET", f"/v1/bookings/{cb['id']}/history", token=wt)
    ok(s == 404, "other diners can't read a booking's history")

print("FAILURES:", fails, "SKIPPED:", skips)
sys.exit(1 if fails else 0)
