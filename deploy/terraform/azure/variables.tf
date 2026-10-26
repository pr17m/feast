variable "country" {
  description = "Country this stack serves (ISO code, lower case): in, au, nz, ca, gb."
  type        = string
  validation {
    condition     = can(regex("^[a-z]{2}$", var.country))
    error_message = "Use a 2-letter country code, e.g. au."
  }
}

variable "location" {
  description = "Azure region close to the users, e.g. australiaeast, centralindia."
  type        = string
}

variable "subscription_id" {
  description = "Azure subscription id (empty = from ARM_SUBSCRIPTION_ID / az CLI)."
  type        = string
  default     = null
}

variable "image_tag" {
  type    = string
  default = "latest"
}

variable "environment" {
  description = "development (test payments, OTP 123456) or production (real providers required)."
  type        = string
  default     = "development"
}

variable "min_replicas" {
  description = "Always-on replicas (1+: live updates and background work need a running instance)."
  type        = number
  default     = 1
}

variable "max_replicas" {
  type    = number
  default = 3
}

variable "postgres_sku" {
  type    = string
  default = "B_Standard_B1ms"
}

variable "redis_sku" {
  description = "Basic (no SLA, cheapest) or Standard (replicated)."
  type        = string
  default     = "Basic"
}

variable "protect_data" {
  description = "Prevent destroying the database/storage by accident (resource locks). Turn on for real data."
  type        = bool
  default     = false
}

variable "stripe_publishable_key" {
  type    = string
  default = ""
}

variable "stripe_secret_key" {
  type      = string
  default   = ""
  sensitive = true
}

variable "razorpay_key_id" {
  type    = string
  default = ""
}

variable "razorpay_key_secret" {
  type      = string
  default   = ""
  sensitive = true
}

variable "twilio_account_sid" {
  type    = string
  default = ""
}

variable "twilio_auth_token" {
  type      = string
  default   = ""
  sensitive = true
}

variable "twilio_service_sid" {
  type    = string
  default = ""
}

variable "extra_env" {
  description = "Extra settings for the API as FEAST__* environment variables, e.g. { FEAST__AUTH__OAUTH__GOOGLE__MODE = \"live\", FEAST__AUTH__OAUTH__GOOGLE__CLIENT_IDS = \"123.apps.googleusercontent.com\" }."
  type        = map(string)
  default     = {}
}

variable "stripe_webhook_secret" {
  description = "Stripe webhook signing secret (whsec_...) for https://<site>/v1/webhooks/payments/stripe. Empty = webhooks off."
  type        = string
  default     = ""
  sensitive   = true
}

variable "razorpay_webhook_secret" {
  description = "Razorpay webhook secret for https://<site>/v1/webhooks/payments/razorpay. Empty = webhooks off."
  type        = string
  default     = ""
  sensitive   = true
}

variable "legal" {
  description = "Business details for the web app's policy pages, as FEAST_LEGAL_* / FEAST_SUPPORT_* env vars (see webapp/src/lib/legal.ts), e.g. { FEAST_LEGAL_NAME = \"Feast Dining Pty Ltd\", FEAST_SUPPORT_EMAIL = \"help@feast.example\" }."
  type        = map(string)
  default     = {}
}

variable "fcm_credentials_json" {
  description = "Firebase service-account JSON for phone app push notifications (FCM HTTP v1). Empty = push off. See deploy/STORE_RELEASE_CHECKLIST.md."
  type        = string
  default     = ""
  sensitive   = true
}

variable "review_numbers" {
  description = "Phone numbers (E.164) that App Store / Google Play reviewers sign in with, using review_code instead of a texted code. Empty = off."
  type        = list(string)
  default     = []
}

variable "review_code" {
  description = "Sign-in code (6+ digits) for review_numbers. Required when review_numbers is set."
  type        = string
  default     = ""
  sensitive   = true
}

variable "ios_app_ids" {
  description = "iOS apps allowed to open this site's links, as \"<TeamID>.<bundle id>\" (served at /.well-known/apple-app-site-association)."
  type        = list(string)
  default     = []
}

variable "android_apps" {
  description = "Android apps allowed to open this site's links, as \"<package>:<SHA-256 fingerprint>\" (several fingerprints separated by |; served at /.well-known/assetlinks.json)."
  type        = list(string)
  default     = []
}
