# Feast on AWS — one country per stack (use a Terraform workspace per country).
#
#   Internet → ALB (HTTP/HTTPS) → ECS Fargate task: [web :3000 → api :8080 (localhost)]
#   api → RDS PostgreSQL, ElastiCache Redis, S3 (documents)
#
# The web container is the only target; it forwards /v1/* to the API in the
# same task. The API runs migrations on start and loads demo data into an
# empty database (development).

locals {
  name   = "feast-${var.country}"
  domain = var.domain_name != "" ? var.domain_name : aws_lb.app.dns_name
  https  = var.certificate_arn != ""
  url    = "${local.https ? "https" : "http"}://${local.domain}"
  prod   = var.environment == "production"
}

data "aws_availability_zones" "available" {
  state = "available"
}

# ------------------------------------------------------------------ network

resource "aws_vpc" "main" {
  cidr_block           = "10.40.0.0/16"
  enable_dns_hostnames = true
  enable_dns_support   = true
  tags                 = { Name = local.name }
}

resource "aws_internet_gateway" "main" {
  vpc_id = aws_vpc.main.id
}

resource "aws_subnet" "public" {
  count                   = 2
  vpc_id                  = aws_vpc.main.id
  cidr_block              = cidrsubnet(aws_vpc.main.cidr_block, 8, count.index)
  availability_zone       = data.aws_availability_zones.available.names[count.index]
  map_public_ip_on_launch = true
  tags                    = { Name = "${local.name}-public-${count.index}" }
}

# Databases live in private subnets (no internet route).
resource "aws_subnet" "private" {
  count             = 2
  vpc_id            = aws_vpc.main.id
  cidr_block        = cidrsubnet(aws_vpc.main.cidr_block, 8, 10 + count.index)
  availability_zone = data.aws_availability_zones.available.names[count.index]
  tags              = { Name = "${local.name}-private-${count.index}" }
}

resource "aws_route_table" "public" {
  vpc_id = aws_vpc.main.id
  route {
    cidr_block = "0.0.0.0/0"
    gateway_id = aws_internet_gateway.main.id
  }
}

resource "aws_route_table_association" "public" {
  count          = 2
  subnet_id      = aws_subnet.public[count.index].id
  route_table_id = aws_route_table.public.id
}

resource "aws_security_group" "alb" {
  name   = "${local.name}-alb"
  vpc_id = aws_vpc.main.id
  ingress {
    from_port   = 80
    to_port     = 80
    protocol    = "tcp"
    cidr_blocks = ["0.0.0.0/0"]
  }
  ingress {
    from_port   = 443
    to_port     = 443
    protocol    = "tcp"
    cidr_blocks = ["0.0.0.0/0"]
  }
  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }
}

resource "aws_security_group" "app" {
  name   = "${local.name}-app"
  vpc_id = aws_vpc.main.id
  ingress {
    from_port       = 3000
    to_port         = 3000
    protocol        = "tcp"
    security_groups = [aws_security_group.alb.id]
  }
  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }
}

resource "aws_security_group" "data" {
  name   = "${local.name}-data"
  vpc_id = aws_vpc.main.id
  ingress {
    from_port       = 5432
    to_port         = 5432
    protocol        = "tcp"
    security_groups = [aws_security_group.app.id]
  }
  ingress {
    from_port       = 6379
    to_port         = 6379
    protocol        = "tcp"
    security_groups = [aws_security_group.app.id]
  }
}

# ------------------------------------------------------------------ images

resource "aws_ecr_repository" "api" {
  name                 = "${local.name}-api"
  force_delete         = !var.protect_data
  image_scanning_configuration {
    scan_on_push = true
  }
}

resource "aws_ecr_repository" "web" {
  name                 = "${local.name}-web"
  force_delete         = !var.protect_data
  image_scanning_configuration {
    scan_on_push = true
  }
}

# ------------------------------------------------------------------ data

resource "random_password" "db" {
  length  = 32
  special = false
}

resource "random_password" "jwt" {
  length  = 48
  special = false
}

resource "random_password" "seed" {
  length  = 20
  special = false
}

resource "aws_db_subnet_group" "main" {
  name       = local.name
  subnet_ids = aws_subnet.private[*].id
}

resource "aws_db_instance" "main" {
  identifier                = local.name
  engine                    = "postgres"
  engine_version            = "16"
  instance_class            = var.db_instance_class
  allocated_storage         = 20
  max_allocated_storage     = 200
  storage_encrypted         = true
  db_name                   = "feast"
  username                  = "feast"
  password                  = random_password.db.result
  db_subnet_group_name      = aws_db_subnet_group.main.name
  vpc_security_group_ids    = [aws_security_group.data.id]
  backup_retention_period   = 7
  deletion_protection       = var.protect_data
  skip_final_snapshot       = !var.protect_data
  final_snapshot_identifier = var.protect_data ? "${local.name}-final" : null
  apply_immediately         = true
}

resource "aws_elasticache_subnet_group" "main" {
  name       = local.name
  subnet_ids = aws_subnet.private[*].id
}

resource "aws_elasticache_cluster" "main" {
  cluster_id         = local.name
  engine             = "redis"
  engine_version     = "7.1"
  node_type          = var.redis_node_type
  num_cache_nodes    = 1
  port               = 6379
  subnet_group_name  = aws_elasticache_subnet_group.main.name
  security_group_ids = [aws_security_group.data.id]
}

resource "aws_s3_bucket" "documents" {
  bucket_prefix = "${local.name}-documents-"
  force_destroy = !var.protect_data
}

resource "aws_s3_bucket_public_access_block" "documents" {
  bucket                  = aws_s3_bucket.documents.id
  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}

resource "aws_s3_bucket_server_side_encryption_configuration" "documents" {
  bucket = aws_s3_bucket.documents.id
  rule {
    apply_server_side_encryption_by_default {
      sse_algorithm = "AES256"
    }
  }
}

# The API signs S3 requests with an access key scoped to this bucket only.
resource "aws_iam_user" "documents" {
  name = "${local.name}-documents"
}

resource "aws_iam_user_policy" "documents" {
  name = "documents"
  user = aws_iam_user.documents.name
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      { Effect = "Allow", Action = ["s3:GetObject", "s3:PutObject"], Resource = "${aws_s3_bucket.documents.arn}/*" },
      { Effect = "Allow", Action = ["s3:ListBucket"], Resource = aws_s3_bucket.documents.arn },
    ]
  })
}

resource "aws_iam_access_key" "documents" {
  user = aws_iam_user.documents.name
}

# ------------------------------------------------------------------ secrets (SSM SecureString)

locals {
  secrets = {
    FEAST__DATABASE__URL                         = "postgres://feast:${random_password.db.result}@${aws_db_instance.main.address}:5432/feast?sslmode=require"
    FEAST__NOTIFICATIONS__DATABASE_URL           = "postgres://feast:${random_password.db.result}@${aws_db_instance.main.address}:5432/feast?sslmode=require"
    FEAST__AUTH__JWT_SECRET                      = random_password.jwt.result
    FEAST__APP__SEED_PASSWORD                    = random_password.seed.result
    FEAST__APP__INITIAL_PARTNER_PASSWORD         = random_password.seed.result
    FEAST__STORAGE__S3__ACCESS_KEY_ID            = aws_iam_access_key.documents.id
    FEAST__STORAGE__S3__SECRET_ACCESS_KEY        = aws_iam_access_key.documents.secret
    FEAST__PAYMENTS__PROVIDERS__STRIPE__SECRET_KEY = var.stripe_secret_key
    FEAST__PAYMENTS__PROVIDERS__RAZORPAY__KEY_SECRET = var.razorpay_key_secret
    FEAST__PAYMENTS__PROVIDERS__STRIPE__WEBHOOK_SECRET   = var.stripe_webhook_secret
    FEAST__PAYMENTS__PROVIDERS__RAZORPAY__WEBHOOK_SECRET = var.razorpay_webhook_secret
    FEAST__AUTH__OTP__TWILIO__AUTH_TOKEN         = var.twilio_auth_token
  }
}

resource "aws_ssm_parameter" "secret" {
  for_each = local.secrets
  name     = "/${local.name}/${each.key}"
  type     = "SecureString"
  # SSM rejects empty values; "-" means "not set" for optional provider keys.
  value = each.value != "" ? each.value : "-"
}

# ------------------------------------------------------------------ load balancer

resource "aws_lb" "app" {
  name               = local.name
  load_balancer_type = "application"
  security_groups    = [aws_security_group.alb.id]
  subnets            = aws_subnet.public[*].id
  idle_timeout       = 300 # live updates (SSE) keep connections open; the API pings every 15s
}

resource "aws_lb_target_group" "web" {
  name        = "${local.name}-web"
  port        = 3000
  protocol    = "HTTP"
  target_type = "ip"
  vpc_id      = aws_vpc.main.id
  health_check {
    path                = "/healthz"
    healthy_threshold   = 2
    unhealthy_threshold = 3
    interval            = 15
  }
  deregistration_delay = 30
}

resource "aws_lb_listener" "http" {
  load_balancer_arn = aws_lb.app.arn
  port              = 80
  protocol          = "HTTP"
  dynamic "default_action" {
    for_each = local.https ? [] : [1]
    content {
      type             = "forward"
      target_group_arn = aws_lb_target_group.web.arn
    }
  }
  dynamic "default_action" {
    for_each = local.https ? [1] : []
    content {
      type = "redirect"
      redirect {
        port        = "443"
        protocol    = "HTTPS"
        status_code = "HTTP_301"
      }
    }
  }
}

resource "aws_lb_listener" "https" {
  count             = local.https ? 1 : 0
  load_balancer_arn = aws_lb.app.arn
  port              = 443
  protocol          = "HTTPS"
  ssl_policy        = "ELBSecurityPolicy-TLS13-1-2-2021-06"
  certificate_arn   = var.certificate_arn
  default_action {
    type             = "forward"
    target_group_arn = aws_lb_target_group.web.arn
  }
}

# ------------------------------------------------------------------ ECS

resource "aws_cloudwatch_log_group" "app" {
  name              = "/ecs/${local.name}"
  retention_in_days = 30
}

resource "aws_ecs_cluster" "main" {
  name = local.name
  setting {
    name  = "containerInsights"
    value = "enabled"
  }
}

resource "aws_iam_role" "execution" {
  name = "${local.name}-execution"
  assume_role_policy = jsonencode({
    Version   = "2012-10-17"
    Statement = [{ Effect = "Allow", Principal = { Service = "ecs-tasks.amazonaws.com" }, Action = "sts:AssumeRole" }]
  })
}

resource "aws_iam_role_policy_attachment" "execution" {
  role       = aws_iam_role.execution.name
  policy_arn = "arn:aws:iam::aws:policy/service-role/AmazonECSTaskExecutionRolePolicy"
}

resource "aws_iam_role_policy" "execution_secrets" {
  name = "read-secrets"
  role = aws_iam_role.execution.id
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect   = "Allow"
      Action   = ["ssm:GetParameters"]
      Resource = [for p in aws_ssm_parameter.secret : p.arn]
    }]
  })
}

locals {
  log = {
    logDriver = "awslogs"
    options = {
      "awslogs-group"         = aws_cloudwatch_log_group.app.name
      "awslogs-region"        = var.region
      "awslogs-stream-prefix" = "app"
    }
  }
  api_env = [
    for k, v in merge({
      FEAST_COUNTRY                                    = var.country
      FEAST_ENV                                        = var.environment
      FEAST__APP__SEED_ON_EMPTY                        = local.prod ? "false" : "true" # demo restaurants/diners in development only
      FEAST__HTTP__PORT                                = "8080"
      FEAST__HTTP__PROXY_HOPS                          = "1"
      FEAST__BUS__KIND                                 = "redis"
      FEAST__BUS__REDIS__URL                           = "redis://${aws_elasticache_cluster.main.cache_nodes[0].address}:6379/0"
      FEAST__STORAGE__KIND                             = "s3"
      FEAST__STORAGE__S3__ENDPOINT                     = "https://s3.${var.region}.amazonaws.com"
      FEAST__STORAGE__S3__BUCKET                       = aws_s3_bucket.documents.bucket
      FEAST__STORAGE__S3__REGION                       = var.region
      FEAST__PAYMENTS__FORCE_PROVIDER                  = local.prod ? "" : "mock"
      FEAST__PAYMENTS__PROVIDERS__MOCK__ENABLED        = local.prod ? "false" : "true"
      FEAST__PAYMENTS__PROVIDERS__STRIPE__ENABLED      = var.stripe_secret_key != "" ? "true" : "false"
      FEAST__PAYMENTS__PROVIDERS__STRIPE__PUBLISHABLE_KEY = var.stripe_publishable_key
      FEAST__PAYMENTS__PROVIDERS__RAZORPAY__ENABLED    = var.razorpay_key_secret != "" ? "true" : "false"
      FEAST__PAYMENTS__PROVIDERS__RAZORPAY__KEY_ID     = var.razorpay_key_id
      FEAST__AUTH__OTP__PROVIDER                       = var.twilio_account_sid != "" ? "twilio" : "fixed"
      FEAST__AUTH__OTP__TWILIO__ACCOUNT_SID            = var.twilio_account_sid
      FEAST__AUTH__OTP__TWILIO__SERVICE_SID            = var.twilio_service_sid
      FEAST__HTTP__CORS_ORIGINS                        = local.url
    }, var.extra_env) : { name = k, value = v }
  ]
  api_secrets = [for k, p in aws_ssm_parameter.secret : { name = k, valueFrom = p.arn }]
}

resource "aws_ecs_task_definition" "app" {
  family                   = local.name
  requires_compatibilities = ["FARGATE"]
  network_mode             = "awsvpc"
  cpu                      = var.task_cpu
  memory                   = var.task_memory
  execution_role_arn       = aws_iam_role.execution.arn
  runtime_platform {
    operating_system_family = "LINUX"
    cpu_architecture        = var.cpu_architecture
  }
  container_definitions = jsonencode([
    {
      name         = "api"
      image        = "${aws_ecr_repository.api.repository_url}:${var.image_tag}"
      command      = ["all"]
      essential    = true
      environment  = local.api_env
      secrets      = local.api_secrets
      portMappings = [{ containerPort = 8080 }]
      healthCheck = {
        command     = ["CMD-SHELL", "bash -c 'exec 3<>/dev/tcp/127.0.0.1/8080 && printf \"GET /healthz HTTP/1.0\\r\\n\\r\\n\" >&3 && grep -q ok <&3'"]
        interval    = 15
        timeout     = 5
        retries     = 3
        startPeriod = 60
      }
      logConfiguration = local.log
    },
    {
      name         = "web"
      image        = "${aws_ecr_repository.web.repository_url}:${var.image_tag}"
      essential    = true
      environment  = concat([{ name = "FEAST_API_INTERNAL_URL", value = "http://127.0.0.1:8080" }, { name = "FEAST_COUNTRY", value = var.country }], [for k, v in var.legal : { name = k, value = v }])
      portMappings = [{ containerPort = 3000 }]
      dependsOn    = [{ containerName = "api", condition = "HEALTHY" }]
      logConfiguration = local.log
    },
  ])
}

resource "aws_ecs_service" "app" {
  name                              = local.name
  cluster                           = aws_ecs_cluster.main.id
  task_definition                   = aws_ecs_task_definition.app.arn
  desired_count                     = var.desired_count
  launch_type                       = "FARGATE"
  health_check_grace_period_seconds = 120
  network_configuration {
    subnets          = aws_subnet.public[*].id
    security_groups  = [aws_security_group.app.id]
    assign_public_ip = true # pulls images from ECR without a NAT gateway
  }
  load_balancer {
    target_group_arn = aws_lb_target_group.web.arn
    container_name   = "web"
    container_port   = 3000
  }
  deployment_circuit_breaker {
    enable   = true
    rollback = true
  }
  depends_on = [aws_lb_listener.http]
}
