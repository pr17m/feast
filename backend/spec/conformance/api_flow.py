"""HTTP conformance test for any Feast backend (Rust today, Go later) serving
Australia with development seed data:

    python3 api_flow.py http://localhost:8080        # exit code 1 on any failure

Covers partner signup, licence uploads (incl. a forged file), admin review,
going live, subscription checkout/decline/payment, admin billing views,
per-country market rules and +61 phone sign-in. FEAST_ADMIN_PASSWORD overrides
the seeded admin password.
"""
import json, os, sys, urllib.request, uuid
BASE = sys.argv[1].rstrip("/")
ADMIN_PASSWORD = os.environ.get("FEAST_ADMIN_PASSWORD", "feast123")
def call(method, path, body=None, token=None, raw=None, ctype=None):
    data = raw if raw is not None else (json.dumps(body).encode() if body is not None else None)
    req = urllib.request.Request(BASE + path, data=data, method=method)
    if data is not None: req.add_header("content-type", ctype or "application/json")
    if token: req.add_header("authorization", "Bearer " + token)
    try:
        with urllib.request.urlopen(req) as r:
            b = r.read(); return r.status, (json.loads(b) if b and r.headers.get_content_type()=="application/json" else b)
    except urllib.error.HTTPError as e:
        return e.code, json.loads(e.read() or b"{}")
def ok(cond, msg):
    print(("PASS " if cond else "FAIL ") + msg)
    if not cond: global fails; fails += 1
fails = 0
s, cities = call("GET", "/v1/cities"); city = cities[0]
# 1. Partner signs up
email = f"owner{uuid.uuid4().hex[:6]}@cafe.test"
s, sess = call("POST", "/v1/partner/signup", {"owner": {"name": "Mia Chen", "email": email, "password": "supersecret"},
  "restaurant": {"name": "Test Bistro", "cityId": city["id"], "area": city["areas"][0], "cuisines": ["Modern Australian"], "costForTwo": 9000, "isPureVeg": False}})
ok(s == 201, f"signup {s} {sess if s!=201 else ''}")
tok = sess["token"]; rid = sess["user"]["restaurantIds"][0]
s, rs = call("GET", "/v1/partner/restaurants", token=tok)
ok(rs[0]["status"] == "ONBOARDING" and rs[0]["currency"] == "AUD", "new restaurant ONBOARDING in AUD")
s, search = call("GET", f"/v1/restaurants?city={city['id']}")
ok(all(r["id"] != rid for r in search["restaurants"]), "not visible to diners yet")
# 2. Compliance
s, c = call("GET", f"/v1/partner/restaurants/{rid}/compliance", token=tok)
reqs = [i["requirement"] for i in c["items"] if i["requirement"]["required"]]
ok(len(reqs) == 4 and not c["readyToSubmit"], f"{len(reqs)} required docs, not ready")
s, e = call("POST", f"/v1/partner/restaurants/{rid}/submit", token=tok)
ok(s == 400, "submit blocked without docs")
pdf = b"%PDF-1.4\n%test\n"
for r in reqs:
    bnd = "XBOUNDARY"
    parts = [f'--{bnd}\r\nContent-Disposition: form-data; name="requirementId"\r\n\r\n{r["id"]}\r\n'.encode()]
    if r.get("numberLabel"): parts.append(f'--{bnd}\r\nContent-Disposition: form-data; name="number"\r\n\r\nNUM123\r\n'.encode())
    if r.get("hasExpiry"): parts.append(f'--{bnd}\r\nContent-Disposition: form-data; name="expiresOn"\r\n\r\n2028-01-01\r\n'.encode())
    parts.append(f'--{bnd}\r\nContent-Disposition: form-data; name="file"; filename="{r["id"]}.pdf"\r\nContent-Type: application/pdf\r\n\r\n'.encode() + pdf + b"\r\n")
    parts.append(f"--{bnd}--\r\n".encode())
    s, c = call("POST", f"/v1/partner/restaurants/{rid}/documents", token=tok, raw=b"".join(parts), ctype=f"multipart/form-data; boundary={bnd}")
    ok(s == 200, f"upload {r['id']} {s} {c if s!=200 else ''}")
# reject non-document
bnd="XB"; body=(f'--{bnd}\r\nContent-Disposition: form-data; name="requirementId"\r\n\r\n{reqs[0]["id"]}\r\n--{bnd}\r\nContent-Disposition: form-data; name="number"\r\n\r\n1\r\n--{bnd}\r\nContent-Disposition: form-data; name="expiresOn"\r\n\r\n2028-01-01\r\n--{bnd}\r\nContent-Disposition: form-data; name="file"; filename="x.pdf"\r\nContent-Type: application/pdf\r\n\r\n<html>evil</html>\r\n--{bnd}--\r\n').encode()
s, e = call("POST", f"/v1/partner/restaurants/{rid}/documents", token=tok, raw=body, ctype=f"multipart/form-data; boundary={bnd}")
ok(s == 400 and e["code"] == "INVALID_FILE_TYPE", f"fake PDF rejected ({e.get('code')})")
s, c = call("POST", f"/v1/partner/restaurants/{rid}/submit", token=tok)
ok(s == 200 and c["restaurantStatus"] == "PENDING_APPROVAL", "submitted for review")
# 3. Admin reviews
s, adm = call("POST", "/v1/auth/login", {"email": "admin@feast.demo", "password": ADMIN_PASSWORD}); at = adm["token"]
s, q = call("GET", "/v1/admin/approvals", token=at)
item = next(i for i in q if i["restaurant"]["id"] == rid)
ok(item["restaurant"]["status"] == "PENDING_APPROVAL", "in approval queue")
s, e = call("POST", f"/v1/admin/restaurants/{rid}/status", {"status": "LIVE"}, token=at)
ok(s == 409 and e["code"] == "COMPLIANCE_INCOMPLETE", "can't go live before documents approved")
docs = [i["document"] for i in item["compliance"]["items"] if i.get("document")]
s, f = call("GET", f"/v1/documents/{docs[0]['id']}/file", token=at)
ok(s == 200 and f.startswith(b"%PDF"), f"admin can open document file {s} {f[:20] if isinstance(f,bytes) else f}")
s, e = call("GET", f"/v1/documents/{docs[0]['id']}/file")
ok(s == 401, "document file needs auth")
s, c = call("POST", f"/v1/admin/documents/{docs[0]['id']}/review", {"decision": "REJECTED"}, token=at)
ok(s == 400, "reject needs a note")
for d in docs:
    s, c = call("POST", f"/v1/admin/documents/{d['id']}/review", {"decision": "APPROVED"}, token=at)
ok(c["complete"], "all documents approved")
s, r = call("POST", f"/v1/admin/restaurants/{rid}/status", {"status": "LIVE"}, token=at)
ok(s == 200 and r["status"] == "LIVE", "approved → LIVE")
s, search = call("GET", f"/v1/restaurants?city={city['id']}")
ok(any(x["id"] == rid for x in search["restaurants"]), "now visible to diners")
# 4. Subscription
s, ov = call("GET", f"/v1/partner/restaurants/{rid}/subscription", token=tok)
ok(ov["subscription"]["status"] == "TRIAL" and ov["subscription"]["daysLeft"] == 30, f"30-day trial started ({ov['subscription']['status']}, {ov['subscription'].get('daysLeft')})")
ok(ov["plans"][0]["currency"] == "AUD" and ov["plans"][0]["total"] == 4950, f"AUD plan with GST: {ov['plans'][0]['total']}")
s, pi = call("POST", f"/v1/partner/restaurants/{rid}/subscription/checkout", {"planId": "MONTHLY"}, token=tok)
ok(s == 201 and pi["amount"] == 4950 and pi["checkout"]["mode"] == "test", f"checkout intent {pi.get('amount')} via {pi.get('provider')}")
s, failed = call("POST", f"/v1/payments/intents/{pi['id']}/confirm", {"method": "CARD", "simulate": "failure"}, token=tok)
ok(failed["status"] == "FAILED", "declined payment recorded")
s, paid = call("POST", f"/v1/payments/intents/{pi['id']}/confirm", {"method": "CARD"}, token=tok)
ok(paid["status"] == "SUCCEEDED", "payment succeeded")
s, ov = call("GET", f"/v1/partner/restaurants/{rid}/subscription", token=tok)
ok(ov["subscription"]["status"] == "ACTIVE" and len(ov["payments"]) == 2, f"ACTIVE with 2 invoices ({ov['subscription']['status']})")
s, pp = call("GET", "/v1/admin/platform-payments", token=at)
mine = [p for p in pp if p["restaurant"]["id"] == rid]
ok(any(p["status"] == "PAID" and p["currency"] == "AUD" and p["invoiceNumber"].startswith("FST-") for p in mine), "admin sees the platform payment")
s, ext = call("POST", f"/v1/admin/restaurants/{rid}/subscription/extend", {"days": 14, "note": "launch promo"}, token=at)
ok(s == 200 and ext["payment"]["planId"] == "COMP", "admin granted free days")
s, m = call("GET", "/v1/admin/metrics", token=at)
ok(m["subscriptions"]["revenue"][0]["currency"] == "AUD" and m["payments"]["byCurrency"][0]["currency"] == "AUD", "metrics per currency")
# 5. Market admin: wrong country refused
s, mk = call("GET", "/v1/markets"); mk = mk[0]
s, e = call("PUT", "/v1/admin/markets/IN", mk, token=at)
ok(s == 400 and e["code"] == "WRONG_COUNTRY", "can't edit another country's market here")
mk["foodTaxPercent"] = 10
s, saved = call("PUT", "/v1/admin/markets/AU", mk, token=at)
ok(s == 200, f"save market {s} {saved if s!=200 else ''}")
# 6. Diner phone validation (AU)
s, e = call("POST", "/v1/auth/otp", {"phone": "+919876543210"})
ok(s == 200 or s == 400, f"otp IN phone on AU deployment -> {s}")
s, r = call("POST", "/v1/auth/otp", {"phone": "+61412345678"})
ok(s == 200, "AU phone accepted")
s, me = call("POST", "/v1/auth/otp/verify", {"phone": "+61412345678", "otp": "123456"})
ok(s == 200 and me["user"]["id"] == "u_jack", "seeded AU diner signs in")
print("FAILURES:", fails)
sys.exit(1 if fails else 0)
