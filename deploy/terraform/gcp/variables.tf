variable "project_id" {
  description = "GCP project id (billing enabled)."
  type        = string
}

variable "country" {
  description = "Country this stack serves (ISO code, lower case): in, au, nz, ca, gb."
  type        = string
  validation {
    condition     = can(regex("^[a-z]{2}$", var.country))
    error_message = "Use a 2-letter country code, e.g. au."
  }
}

variable "region" {
  description = "Region close to the users, e.g. australia-southeast1 (Sydney), asia-south1 (Mumbai)."
  type        = string
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

variable "min_instances" {
  description = "Always-on instances (1+: live updates and background work need a running instance)."
  type        = number
  default     = 1
}

variable "max_instances" {
  type    = number
  default = 3
}

variable "sql_tier" {
  type    = string
  default = "db-f1-micro"
}

variable "protect_data" {
  description = "Deletion protection for the database and keep the documents bucket on destroy."
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
