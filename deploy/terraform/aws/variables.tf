variable "country" {
  description = "Country this stack serves (ISO code, lower case): in, au, nz, ca, gb. Loads config/countries/<country>.yaml."
  type        = string
  validation {
    condition     = can(regex("^[a-z]{2}$", var.country))
    error_message = "Use a 2-letter country code, e.g. au."
  }
}

variable "region" {
  description = "AWS region close to the country's users, e.g. ap-southeast-2 (Sydney), ap-south-1 (Mumbai)."
  type        = string
}

variable "image_tag" {
  description = "Tag of the feast-api and feast-web images pushed to this stack's ECR repositories."
  type        = string
  default     = "latest"
}

variable "environment" {
  description = "development (test payments, OTP 123456, test social login) or production (real providers required)."
  type        = string
  default     = "development"
}

variable "domain_name" {
  description = "Optional custom domain (e.g. au.feast.example). Needs certificate_arn; point a CNAME/alias at the load balancer."
  type        = string
  default     = ""
}

variable "certificate_arn" {
  description = "Optional ACM certificate ARN (same region) to serve HTTPS on the load balancer."
  type        = string
  default     = ""
}

variable "cpu_architecture" {
  description = "ARM64 (Graviton: cheaper; native builds on Apple Silicon) or X86_64. Images must be built for it."
  type        = string
  default     = "ARM64"
}

variable "desired_count" {
  description = "Number of app tasks (each runs the web app + API). 2+ for high availability."
  type        = number
  default     = 1
}

variable "task_cpu" {
  type    = number
  default = 1024
}

variable "task_memory" {
  type    = number
  default = 2048
}

variable "db_instance_class" {
  type    = string
  default = "db.t4g.micro"
}

variable "redis_node_type" {
  type    = string
  default = "cache.t4g.micro"
}

variable "protect_data" {
  description = "Deletion protection + final snapshot for the database, and keep the documents bucket on destroy. Turn on for real data."
  type        = bool
  default     = false
}

# ---- optional real providers (leave empty in development) ----

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
