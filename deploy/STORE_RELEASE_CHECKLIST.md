# Store release checklist (server side)

Everything the **server and web app** must have before the Feast iOS and Android
apps are submitted to the App Store and Google Play. The app builds themselves
(signing, store listings, screenshots) are covered in `mobile/`.

Each item gives the config key, the environment variable for Docker Compose
(`deploy/compose/<country>.env`) and the Terraform variable
(`TF_VAR_<name>=… ./deploy.sh <country>` or `<country>.tfvars`). Secrets go in
env vars or secret stores only — never in the database, the admin UI or git.

## 1. Account deletion (both stores require it)

Nothing to configure: it ships with the API (migration `0010_push_tokens_account_deletion`).

- [ ] **In the app:** Account → *Delete my account* (diners) and Partner → Settings → *Delete my account* (owners and staff). `DELETE /v1/me` with `{confirm:"DELETE", otp | password}`.
- [ ] **Public page:** `https://<site>/account-deletion`. Paste this URL into Google Play Console → *App content → Data safety → Delete account URL* and App Store Connect's review notes.
- [ ] **Privacy policy URL:** `https://<site>/legal/privacy`. **Terms:** `https://<site>/legal/terms`. Set `FEAST_LEGAL_*` (compose) / `legal = {…}` (Terraform) and `FEAST_LEGAL_REVIEWED=true` once a lawyer has approved the text, so the pages don't show the draft notice.
- [ ] Rules to describe in review notes if asked: admins can't delete themselves; an owner with open counter payments or open cases is asked to resolve them first; restaurants they solely own are paused; receipts, reviews ("Former Feast diner") and cases are kept without personal details.

## 2. Push notifications (Firebase Cloud Messaging, both platforms)

| Setting | Config key | Compose env | Terraform |
|---|---|---|---|
| Provider | `push.provider` = `fcm` (`""` off, `log` dev only — refused in production) | `PUSH_PROVIDER=fcm` | set automatically when `fcm_credentials_json` is non-empty |
| Service account | `push.fcm.credentials_json` (or `push.fcm.credentials_file`) | `FCM_CREDENTIALS_JSON` (one line) → `FEAST__PUSH__FCM__CREDENTIALS_JSON` | `fcm_credentials_json` (stored as a secret) |

- [ ] Create a Firebase project; add the iOS and Android apps (bundle IDs / package names of the diner and partner apps).
- [ ] Upload the **APNs auth key** (.p8) in Firebase → Project settings → Cloud Messaging, so FCM can deliver to iPhones.
- [ ] Create a service account with the *Firebase Cloud Messaging API Admin* role, download its JSON key, and set it as above. The API refuses to start with `provider=fcm` and no credentials.
- [ ] Check: the API log shows "push notifications via Firebase Cloud Messaging" with your project ID at start-up; sign in on a phone, then `GET /v1/me/push-settings` returns `{enabled:true}` and a booking produces a notification on the phone.
- API used by the apps: `POST /v1/me/push-tokens {platform:"ios"|"android", token, app:"diner"|"partner"}`, `DELETE /v1/me/push-tokens/{token}`, `GET|PUT /v1/me/push-settings`. Tokens FCM reports as unregistered are removed automatically.

## 3. Reviewer sign-in (App Review and Play review can't receive texts)

| Setting | Config key | Compose env | Terraform |
|---|---|---|---|
| Numbers | `auth.otp.review_numbers` (E.164 list) | `REVIEW_NUMBERS=+61400000000` → `FEAST__AUTH__OTP__REVIEW_NUMBERS` | `review_numbers = ["+61400000000"]` |
| Code | `auth.otp.review_code` (6+ digits, not starting with 0, not the dev fixed code) | `REVIEW_CODE` → `FEAST__AUTH__OTP__REVIEW_CODE` | `review_code` (secret) |

- [ ] Pick a number nobody owns (or one you control), set both values, and give the number and code to the reviewers in App Store Connect (*App Review Information → Sign-in required*) and Play Console (*App access*).
- [ ] No text message is sent for these numbers. Every reviewer sign-in is written to the audit log (`auth.review_sign_in`, last three digits only).
- [ ] For the **partner app**, also create a reviewer restaurant account (owner email + password) in the admin app and give those details to the reviewers.
- [ ] Remove the numbers (or rotate the code) after approval if you don't want them to keep working.

## 4. App links (open `https://<site>/…` links in the app)

| Setting | Web env | Terraform |
|---|---|---|
| iOS apps | `FEAST_IOS_APP_IDS=TEAMID.com.feast.diner,TEAMID.com.feast.partner` | `ios_app_ids = [...]` |
| Android apps | `FEAST_ANDROID_APPS=com.feast.diner:<SHA-256>,com.feast.partner:<SHA-256>` (several fingerprints: separate with `\|`) | `android_apps = [...]` |

- [ ] Use the **Play App Signing** certificate's SHA-256 (Play Console → *App integrity*), plus the upload key's if you test sideloaded builds.
- [ ] Check `https://<site>/.well-known/apple-app-site-association` and `https://<site>/.well-known/assetlinks.json` return JSON (404 means the env var is empty or invalid). `/v1/*` and `/admin/*` always stay on the web.
- [ ] Served over HTTPS with no redirect; the iOS app's *Associated Domains* lists `applinks:<site>`.

## 5. Subscriptions are bought on the web only

Nothing to configure. In the apps the partner subscription page shows the plan
and status with "Manage your plan on the Feast website" and no purchase buttons
or upsell banners (Apple 3.1.1 / Google Play payments policy). Diner bill
payments are for real-world services and stay in the app.

## 6. Before you submit

- [ ] `FEAST_ENV=production` and the API starts cleanly (configuration is validated at start-up and lists every problem).
- [ ] Real SMS sign-in (`auth.otp.provider=twilio`) and live payment keys are set; test payments are off.
- [ ] Support contact (`FEAST_SUPPORT_EMAIL`, `FEAST_SUPPORT_PHONE`) matches the store listing.
- [ ] Data safety (Play) / privacy nutrition labels (Apple) match the privacy policy: name, phone, email, payment status (card data goes to Stripe/Razorpay), approximate location from IP, push token, photos you choose to upload.
