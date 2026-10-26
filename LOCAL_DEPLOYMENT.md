# Run Feast locally with Docker (Linux, Windows, macOS)

This guide is for someone running Feast for the first time. By the end, the
full Feast stack for one country will be running on your machine, with demo
restaurants and test payments, so you can click through it as a diner, a
restaurant owner and an admin.

You need about **30 minutes** the first time. Most of that is a one-time build.

> **Short version** (if you already have Docker and Git):
>
> ```sh
> git clone https://github.com/pr17m/feast.git
> cd feast/deploy/compose
> cp au.env.example au.env          # then change the 3 passwords inside
> docker compose --env-file au.env up -d --build
> # open http://localhost:8081
> ```

---

## Contents

1. [What you get](#1-what-you-get)
2. [Before you start](#2-before-you-start)
3. [Install Docker](#3-install-docker): [macOS](#macos) · [Windows](#windows) · [Linux](#linux)
4. [Get the code](#4-get-the-code)
5. [Create your settings file](#5-create-your-settings-file)
6. [Build and start Feast](#6-build-and-start-feast)
7. [Check that it works](#7-check-that-it-works)
8. [Try it out (test walkthrough)](#8-try-it-out-test-walkthrough)
9. [Run India as well](#9-run-india-as-well)
10. [Optional: use a Postgres you already have](#10-optional-use-a-postgres-you-already-have)
11. [Optional: share a public link](#11-optional-share-a-public-link)
12. [Everyday commands](#12-everyday-commands)
13. [Update to the latest code](#13-update-to-the-latest-code)
14. [Reset or remove everything](#14-reset-or-remove-everything)
15. [Troubleshooting](#15-troubleshooting)
16. [Safety notes](#16-safety-notes)

---

## 1. What you get

Feast runs **one stack per country**. Each stack has its own database, Redis
and file storage. Australia (`au`) and India (`in`) are set up out of the box.

```
            ┌───────────────────────── one country (e.g. au) ─────────────────────────┐
 browser ──▶│ web  (Next.js)  ──/v1/*──▶  api (Rust)  ──▶  postgres   (database)      │
 :8081      │ the only open port            │   ▲          redis      (event bus)     │
            │                               └───┘ worker   files      (uploads)       │
            └─────────────────────────────────────────────────────────────────────────┘
```

| Service    | What it does                                                             |
|------------|--------------------------------------------------------------------------|
| `postgres` | The database. Bundled by default; you can use your own (section 10).     |
| `redis`    | Message bus between the API and the worker.                              |
| `migrate`  | Creates/updates the database tables, then exits. This is expected.       |
| `api`      | The Rust API. Loads demo restaurants the first time it starts.           |
| `worker`   | Background jobs: notifications, reminders.                               |
| `web`      | The web app. The only port you open in the browser. It forwards `/v1/*` to the API. |
| `tunnel`   | Optional. A public `https://…trycloudflare.com` link (section 11).       |

All of this is defined in [`deploy/compose/docker-compose.yml`](deploy/compose/docker-compose.yml).

**Local addresses**

| Country   | Settings file | Address                 |
|-----------|---------------|-------------------------|
| Australia | `au.env`      | http://localhost:8081   |
| India     | `in.env`      | http://localhost:8082   |

The stack starts in **development mode**: payments are simulated (no money
moves), the phone sign-in code is always `123456`, and no SMS is sent.

---

## 2. Before you start

### Machine

| Item       | Minimum                          | Recommended               |
|------------|----------------------------------|---------------------------|
| CPU        | 64-bit, 2 cores (x86-64 or ARM64)| 4+ cores                  |
| Memory     | 8 GB in the machine              | 16 GB                     |
| Memory given to Docker | **6 GB**             | 8 GB                      |
| Free disk  | 15 GB                            | 25 GB                     |
| Internet   | Needed for the first build (downloads images, Rust crates and npm packages) | |

The first build compiles the Rust API. It needs the memory above. With less,
the build stops with an "out of memory" error (see [Troubleshooting](#15-troubleshooting)).

### Software

| Tool               | Version          | Why                                       |
|--------------------|------------------|-------------------------------------------|
| Docker Engine      | 20.10 or newer   | Runs the containers                       |
| Docker Compose     | **v2.20 or newer** (the `docker compose` command, not `docker-compose`) | Starts the stack |
| Git                | any recent       | Gets the code                             |
| A web browser      | Chrome, Edge, Firefox or Safari |                            |
| Python 3 (optional)| 3.8+             | Only for the automated API test (section 8) |

Docker Desktop (macOS, Windows) includes Docker Engine and Compose.

### Free ports

Feast uses port **8081** (Australia) and **8082** (India) on your machine.
Nothing else is exposed. If one of these is taken, you can change it (section 5).

---

## 3. Install Docker

Pick your operating system. When you finish, `docker compose version` must
print **v2.20 or newer**.

### macOS

1. Download **Docker Desktop** from https://www.docker.com/products/docker-desktop/.
   Pick the right chip:
   - **Apple silicon** (M1, M2, M3, M4…): "Mac with Apple chip"
   - **Intel**: "Mac with Intel chip"

   Not sure? Apple menu → **About This Mac** → look at "Chip" or "Processor".
2. Open the `.dmg`, drag Docker to Applications, start it, and accept the terms.
   Wait until the whale icon in the menu bar says **Docker Desktop is running**.
3. Give Docker enough memory: Docker Desktop → **Settings** (gear icon) →
   **Resources** → set **Memory to at least 6 GB** → **Apply & restart**.
4. Open **Terminal** and check:

   ```sh
   docker version
   docker compose version
   git --version        # if macOS asks to install developer tools, click Install
   ```

> **Alternatives:** [OrbStack](https://orbstack.dev) and [Colima](https://github.com/abiosoft/colima)
> also work. They support `docker compose` and `host.docker.internal`. With
> Colima, start it with enough memory: `colima start --memory 8 --cpu 4`.

### Windows

Docker on Windows runs Linux containers inside **WSL 2** (Windows Subsystem
for Linux). You need **Windows 10 22H2 or Windows 11**, 64-bit, with
virtualization turned on in the BIOS (it usually is).

1. **Install WSL 2.** Open **PowerShell as Administrator** and run:

   ```powershell
   wsl --install
   ```

   Restart when asked. After the restart, an **Ubuntu** window opens. Pick a
   Linux username and password. (If WSL was already installed, run
   `wsl --update` instead.)
2. **Install Docker Desktop** from https://www.docker.com/products/docker-desktop/
   ("Download for Windows – AMD64", or ARM64 on Snapdragon laptops).
   During setup, keep **Use WSL 2 instead of Hyper-V** ticked.
3. Start Docker Desktop and wait for **Engine running** (bottom-left).
4. Docker Desktop → **Settings** → **Resources** → **WSL integration** →
   turn on **Ubuntu** → **Apply & restart**.
5. **Give WSL enough memory** (only if your PC has 8–12 GB RAM). WSL uses up
   to half of your RAM by default. To raise it, create the file
   `C:\Users\<you>\.wslconfig` with:

   ```ini
   [wsl2]
   memory=6GB
   ```

   Then run `wsl --shutdown` in PowerShell and restart Docker Desktop.
6. **Choose where to run the commands.** There are two ways. Pick one and use
   it for the rest of this guide.

   | | **A. Ubuntu (WSL) terminal — recommended** | **B. PowerShell** |
   |---|---|---|
   | Where the code lives | Inside Linux, e.g. `~/feast` | On the C: drive, e.g. `C:\dev\feast` |
   | Build speed | Fast | Slower (files cross from Windows into Linux) |
   | Commands in this guide | Use the **Linux / macOS** versions as written | Use the **PowerShell** versions |
   | Helper scripts (`redeploy.sh`, `share-url.sh`) | Work as-is | Need Git Bash, or use the PowerShell commands given instead |

   For option A, open **Ubuntu** from the Start menu, then install Git:

   ```sh
   sudo apt update && sudo apt install -y git
   ```

   For option B, install **Git for Windows** from https://git-scm.com/download/win
   (keep the defaults; it includes Git Bash).
7. Check, in the terminal you chose:

   ```sh
   docker version
   docker compose version
   git --version
   ```

> **Do not** put the code under `/mnt/c/...` when using the Ubuntu terminal.
> It works, but the build is many times slower. Keep it in your Linux home
> folder (`~`).

### Linux

Use **Docker Engine** with the **Compose plugin** from Docker's own packages.
Distribution packages (`docker.io`, `docker-compose`) are often too old.

**Ubuntu / Debian (and Mint, Pop!\_OS)**, using Docker's convenience script:

```sh
curl -fsSL https://get.docker.com -o get-docker.sh
sudo sh get-docker.sh
```

**Fedora / RHEL / Rocky / Alma, Arch, others:** follow
https://docs.docker.com/engine/install/ for your distribution. Make sure the
`docker-compose-plugin` package is installed.

Then, on every distribution:

```sh
# start Docker now and on every boot
sudo systemctl enable --now docker

# let your user run docker without sudo (log out and back in afterwards)
sudo usermod -aG docker $USER

# after logging back in:
docker version
docker compose version
git --version          # install git with your package manager if missing
```

Notes:

- Docker Engine on Linux has no memory limit of its own. It can use all of
  the machine's RAM. On a machine with less than 8 GB, add swap before the
  first build (see [Troubleshooting](#15-troubleshooting)).
- **Rootless Docker** and **Podman** are not tested with this setup. Use the
  normal Docker Engine.
- SELinux (Fedora/RHEL) needs no extra setup: Feast stores its data in Docker
  volumes, not in folders on your disk.

---

## 4. Get the code

**Linux, macOS, Windows (Ubuntu terminal):**

```sh
cd ~
git clone https://github.com/pr17m/feast.git
cd feast/deploy/compose
```

**Windows (PowerShell):**

```powershell
mkdir C:\dev -Force; cd C:\dev
git clone https://github.com/pr17m/feast.git
cd feast\deploy\compose
```

If the repository is private, GitHub asks you to sign in. You can also clone
over SSH: `git clone git@github.com:pr17m/feast.git`.

> **Windows line endings.** The repository has a `.gitattributes` file that
> keeps shell scripts and config files with Linux line endings, so they work
> inside the containers. If you cloned an old copy before that file existed and
> see `bash\r: No such file or directory`, see [Troubleshooting](#15-troubleshooting).

From here on, every command runs **inside the `deploy/compose` folder**.

---

## 5. Create your settings file

Each country has an example settings file. Copy it, then edit the copy.
Your copy (`au.env`) is ignored by Git, so your passwords are never committed.

**Linux / macOS / Ubuntu terminal:**

```sh
cp au.env.example au.env
```

**Windows (PowerShell):**

```powershell
Copy-Item au.env.example au.env
```

### Change the three passwords

Open `au.env` in any text editor (`nano au.env`, `code au.env`, `notepad au.env`
or TextEdit) and replace these three values:

| Setting             | What it is                                                     | Example of a good value |
|---------------------|----------------------------------------------------------------|-------------------------|
| `POSTGRES_PASSWORD` | Password of the bundled database                               | 32+ random characters   |
| `JWT_SECRET`        | Key used to sign sign-in tokens. **32 characters or more.**    | 48 random characters    |
| `SEED_PASSWORD`     | Password for the demo **owner** and **admin** accounts. You will type this to sign in. | something you can remember |

To generate random values:

```sh
# Linux / macOS / Ubuntu terminal / Git Bash
openssl rand -hex 24
```

```powershell
# Windows PowerShell
-join ((1..48) | ForEach-Object { '{0:x}' -f (Get-Random -Maximum 16) })
```

Example result (do not copy these values):

```ini
POSTGRES_PASSWORD=9f2c1e7a54b0d3c8e6f1a2b4c7d9e0f3a5b6c8d1e2f4a7b9
JWT_SECRET=4e8a1c7f2b9d0e3a6c5f8b1d4e7a0c3f6b9d2e5a8c1f4b7e
SEED_PASSWORD=MyFeastTest2026
```

> Set `POSTGRES_PASSWORD` **before the first start** and then leave it alone.
> The database keeps the password it was created with. Changing it later
> breaks the connection (see [Troubleshooting](#15-troubleshooting)).

### Other settings (leave as they are for a first test)

| Setting             | Default        | Meaning                                                   |
|---------------------|----------------|-----------------------------------------------------------|
| `FEAST_COUNTRY`     | `au`           | Which country this stack runs                             |
| `PORT`              | `8081`         | The port you open in the browser. Change it if 8081 is busy. |
| `FEAST_ENV`         | `development`  | Test payments, sign-in code `123456`, demo social login   |
| `COMPOSE_PROFILES`  | `db`           | Run the bundled Postgres. Remove it to use your own (section 10). |

Everything else in the file (real payments, SMS, photos, legal pages, phone
apps) is commented out and optional. You don't need it to test Feast.

---

## 6. Build and start Feast

```sh
docker compose --env-file au.env up -d --build
```

What happens:

1. Docker downloads the base images (Postgres, Redis, Rust, Node).
2. It **builds the API** (compiles Rust). This is the slow step:
   about **10 minutes** on a recent Mac, **15–30 minutes** on smaller laptops.
   It only happens the first time and after code changes.
3. It builds the web app (2–4 minutes).
4. It starts Postgres and Redis, runs `migrate` (creates the tables), then
   starts `api`, `worker` and `web`.
5. On its first start, the API loads demo restaurants, menus, owners and an
   admin.

You can follow progress in a second terminal:

```sh
docker compose --env-file au.env logs -f
```

Press `Ctrl+C` to stop watching. This doesn't stop Feast.

> **`--env-file au.env` is needed on every command.** It tells Docker which
> country you mean. Without it you get `set FEAST_COUNTRY in the env file`.

---

## 7. Check that it works

**1. All services are up:**

```sh
docker compose --env-file au.env ps
```

You should see `postgres`, `redis`, `api`, `worker` and `web` with status
`Up` (`api`, `worker` and `web` show `(healthy)` after about 20 seconds). `migrate` is
not listed, or shows `Exited (0)`. That is correct: it runs once and stops.

To also list stopped services: `docker compose --env-file au.env ps -a`.

**2. The API answers through the web app:**

```sh
# Linux / macOS / Ubuntu terminal
curl -s http://localhost:8081/v1/markets
```

```powershell
# Windows PowerShell (use curl.exe, not curl)
curl.exe -s http://localhost:8081/v1/markets
```

You should get JSON describing the Australian market (currency `AUD`, etc.).

**3. Open the site:** http://localhost:8081

You should see the Feast home page with Sydney restaurants.

---

## 8. Try it out (test walkthrough)

### Accounts

| Role              | How to sign in                                                                                  |
|-------------------|-------------------------------------------------------------------------------------------------|
| Diner             | Any valid Australian mobile, e.g. `412 345 678`. The code is **`123456`** (no SMS is sent).     |
| Diner (social)    | "Continue with Google / Apple / Facebook" runs in test mode: type any name and email.           |
| Restaurant owner  | `owner.au@feast.demo` with your `SEED_PASSWORD`                                                 |
| Admin             | `admin@feast.demo` with your `SEED_PASSWORD`                                                    |

For India (section 9): diner phone e.g. `98765 43210`, owner `owner@feast.demo`.

### Suggested test path

Use two browser windows (one normal, one private/incognito) so you can be a
diner and an owner at the same time.

1. **Diner: book a table.** Open a restaurant, pick a date, time and party
   size, and book. Pay with the test checkout. In development mode you can
   choose to succeed or to simulate a declined payment.
2. **Owner: see the booking.** Sign in as `owner.au@feast.demo` and open
   **Partner → Bookings**. The new booking is there. The notification bell
   updates live.
3. **Owner: edit the listing.** Try **Menu** (upload a dish photo: JPG, PNG or
   WebP up to 5 MB), **Offers** and **Listing**.
4. **New restaurant: onboard it.** Signed out, choose **"List it on Feast"**,
   sign up, and upload the licence documents (any small PDF or image works
   for testing).
5. **Admin: approve it.** Sign in as `admin@feast.demo`, open
   **Admin → Approvals**, review each document and approve. The restaurant
   goes live with a 30-day trial and appears in search.
6. **Diner: pay the bill.** On the day of a booking, open the restaurant page,
   choose **"Pay bill & get your deal"**, type a bill total and pay.

### Automated API check (optional)

A built-in script runs about 30 API checks against the Australia stack
(partner signup, document review, going live, subscription payments, phone
sign-in). It only needs Python 3, no extra packages. Run it from the
**repository root**:

```sh
# Linux / macOS / Ubuntu terminal — use your SEED_PASSWORD
cd ../..
FEAST_ADMIN_PASSWORD='MyFeastTest2026' python3 backend/spec/conformance/api_flow.py http://localhost:8081
```

```powershell
# Windows PowerShell
cd ..\..
$env:FEAST_ADMIN_PASSWORD = 'MyFeastTest2026'
python backend\spec\conformance\api_flow.py http://localhost:8081
```

Each line prints `PASS` or `FAIL`. The exit code is non-zero if anything
failed. The script adds test restaurants to your database; reset it
afterwards if you want a clean demo (section 14).

---

## 9. Run India as well

India is a completely separate stack with its own database. It can run at
the same time as Australia.

```sh
cp in.env.example in.env          # PowerShell: Copy-Item in.env.example in.env
# edit the three passwords in in.env, as in section 5
docker compose --env-file in.env up -d --build
```

Open http://localhost:8082. The second build is fast because both countries
use the same images.

Docker keeps the two stacks apart by name (`feast-au` and `feast-in`), so
their data never mixes.

---

## 10. Optional: use a Postgres you already have

By default the stack runs its own Postgres. To use one already running on
your computer instead:

1. Create one database per country, for example `feast` (Australia) and
   `feast_in` (India):

   ```sh
   psql -U postgres -c "create database feast"
   ```

2. In `au.env`, **delete or comment out** these two lines:

   ```ini
   # COMPOSE_PROFILES=db
   # POSTGRES_PASSWORD=...
   ```

3. Add your connection string. `host.docker.internal` means "this computer"
   when seen from inside Docker. It works on macOS, Windows and Linux with
   this setup.

   ```ini
   DATABASE_URL=postgres://postgres:YOUR_PASSWORD@host.docker.internal:5432/feast
   ```

4. Start as usual: `docker compose --env-file au.env up -d --build`.

**Extra steps on Linux only.** A Postgres installed on the Linux host usually
only listens on `127.0.0.1`, which containers cannot reach. Allow Docker's
network:

- In `postgresql.conf`: `listen_addresses = '*'` (or `'localhost,172.17.0.1'`)
- In `pg_hba.conf`, add: `host  all  all  172.16.0.0/12  scram-sha-256`
- Restart Postgres: `sudo systemctl restart postgresql`
- If you use a firewall (`ufw`, `firewalld`), allow port 5432 from
  `172.16.0.0/12`.

If your Postgres itself runs in Docker and publishes port 5432, no extra
steps are needed.

---

## 11. Optional: share a public link

To let someone else try your local stack over the internet (for example, a
tester in Australia), start the free Cloudflare quick tunnel. No account is
needed.

1. In `au.env`, add `share` to the `COMPOSE_PROFILES` line:

   ```ini
   COMPOSE_PROFILES=db,share        # or just "share" if you use your own Postgres
   ```

   > Use the settings file, not `--profile share` on the command line. A
   > `--profile` flag replaces `COMPOSE_PROFILES`, so the bundled database
   > would be left out.

2. Start it and print the link:

   ```sh
   docker compose --env-file au.env up -d
   ./share-url.sh au.env             # prints https://<random-words>.trycloudflare.com
   ```

On Windows PowerShell (without Git Bash), find the link in the tunnel logs:

```powershell
docker compose --env-file au.env logs tunnel | Select-String trycloudflare.com
```

- The link works over HTTPS while your computer is on and Docker is running.
- It changes every time the tunnel restarts.
- To stop sharing: remove `share` from `COMPOSE_PROFILES` again and run
  `docker compose --env-file au.env stop tunnel`.
- Anyone with the link can use the site, including the demo owner and admin
  accounts. **Set a strong `SEED_PASSWORD` before you share.**

---

## 12. Everyday commands

All commands run in `deploy/compose`. Replace `au.env` with `in.env` for India.

| Task                               | Command                                                    |
|------------------------------------|------------------------------------------------------------|
| Start (after a reboot, etc.)       | `docker compose --env-file au.env up -d`                   |
| Stop (keeps all data)              | `docker compose --env-file au.env down`                    |
| Status                             | `docker compose --env-file au.env ps -a`                   |
| All logs, live                     | `docker compose --env-file au.env logs -f`                 |
| API and worker logs only           | `docker compose --env-file au.env logs -f api worker`      |
| Last 100 lines of the API log      | `docker compose --env-file au.env logs --tail 100 api`     |
| Restart one service                | `docker compose --env-file au.env restart api`             |
| Open a database shell (bundled DB) | `docker compose --env-file au.env exec postgres psql -U feast -d feast` |

Services are set to `restart: unless-stopped`: if you didn't stop them, they
come back by themselves when Docker starts.

---

## 13. Update to the latest code

```sh
git pull
./redeploy.sh            # Australia
./redeploy.sh in         # India
```

`redeploy.sh` rebuilds the images, restarts the stack (migrations run first,
data is kept), waits until Feast answers, and removes old images. Options:
`--no-cache` for a clean rebuild, `--photos` to refresh the demo restaurants'
photos.

Without bash (Windows PowerShell), do the same by hand:

```powershell
git pull
docker compose --env-file au.env up -d --build --remove-orphans
docker image prune -f
```

---

## 14. Reset or remove everything

| Goal                                                     | Command                                                    |
|----------------------------------------------------------|------------------------------------------------------------|
| Stop, keep data                                          | `docker compose --env-file au.env down`                    |
| **Delete all data** and start fresh with the demo data   | `docker compose --env-file au.env down -v` then `up -d`    |
| Also delete the images (Feast, Postgres, Redis)          | `docker compose --env-file au.env down -v --rmi all`       |
| Free disk used by build caches (all Docker projects)     | `docker builder prune`                                     |

`down -v` deletes this country's database, Redis data and uploaded files.
It does **not** touch a Postgres of your own (section 10). To reset that one,
drop and re-create its database.

---

## 15. Troubleshooting

Start with the logs: `docker compose --env-file au.env logs --tail 100 migrate api web`.

| What you see | Cause | Fix |
|---|---|---|
| `Cannot connect to the Docker daemon` / `docker: command not found` | Docker isn't running or isn't installed | macOS/Windows: start Docker Desktop and wait for "running". Linux: `sudo systemctl start docker`. |
| `permission denied ... /var/run/docker.sock` (Linux) | Your user isn't in the `docker` group | `sudo usermod -aG docker $USER`, then log out and back in. |
| `docker-compose: command not found`, or `unknown flag: --env-file` | Old Compose v1 | Use `docker compose` (with a space). Update Docker so `docker compose version` shows v2.20+. |
| `services.depends_on.required` / `Additional property required is not allowed` | Compose older than v2.20 | Update Docker Desktop, or on Linux `sudo apt install docker-compose-plugin` from Docker's repo. |
| `required variable FEAST_COUNTRY is missing` / `set JWT_SECRET` / `set SEED_PASSWORD` | `--env-file` missing, or the setting is empty | Add `--env-file au.env` and check the file has the three passwords. |
| `Bind for 0.0.0.0:8081 failed: port is already allocated` | Something else uses the port | Change `PORT=` in `au.env` (e.g. `8091`) and run `up -d` again. Find the culprit: `lsof -i :8081` (macOS/Linux) or `netstat -ano \| findstr 8081` (Windows). |
| Build stops with `signal: 9, SIGKILL`, `killed`, or `ResourceExhausted` while compiling Rust | Not enough memory for the build | Give Docker at least 6 GB (section 3). Linux: add swap: `sudo fallocate -l 4G /swapfile && sudo chmod 600 /swapfile && sudo mkswap /swapfile && sudo swapon /swapfile`. Then run the same `up -d --build` again; finished steps are cached. |
| `no space left on device` | Docker's disk is full | `docker system prune` (removes stopped containers and unused images), then `docker builder prune`. Docker Desktop: raise the disk limit in Settings → Resources. |
| Build fails downloading crates or npm packages (`timed out`, `certificate`, `407`) | Network, VPN or corporate proxy | Try another network or turn off the VPN. Behind a proxy, set it in Docker Desktop → Settings → Resources → Proxies (Linux: [daemon proxy docs](https://docs.docker.com/engine/daemon/proxy/)). |
| `migrate` exits with `password authentication failed` | `POSTGRES_PASSWORD` was changed after the database was created | Put the old password back, **or** wipe and recreate: `docker compose --env-file au.env down -v` then `up -d`. |
| `migrate` exits with `connection refused` to `host.docker.internal` | Using your own Postgres and it isn't reachable | Check it's running, the database exists, and on Linux follow the extra steps in section 10. |
| `api` keeps restarting | Check `logs api`. With `FEAST_ENV=production` the API refuses unsafe settings and lists what to fix. | For local testing keep `FEAST_ENV=development`. |
| Page loads but shows errors, or `/v1/markets` returns `502` | The API is still starting, or crashed | Wait 30 seconds; check `ps` and `logs api`. |
| Old version of the page after an update | The web app is installable (PWA) and caches pages | Hard refresh: `Ctrl+Shift+R` (`Cmd+Shift+R` on Mac), or open in a private window. |
| `/usr/bin/env: 'bash\r': No such file or directory` | Scripts checked out with Windows line endings | Run `git config --global core.autocrlf input`, then from the repo root: `git rm --cached -r . -q && git reset --hard`. Or use the PowerShell commands in sections 11 and 13. |
| `./redeploy.sh: Permission denied` | The script isn't executable | `chmod +x redeploy.sh share-url.sh` or run `bash redeploy.sh`. |
| Windows build is very slow | Code is on the C: drive (or `/mnt/c`) | Clone inside the Ubuntu terminal under `~` (section 3, option A). |
| `share-url.sh` prints "No tunnel URL yet" | Tunnel still starting, or blocked by your network | Wait 30 seconds and run it again; check `docker compose --env-file au.env logs tunnel`. |
| Database missing after starting the tunnel (`migrate` can't reach `postgres`) | `--profile share` on the command line replaced `COMPOSE_PROFILES=db` | Put `COMPOSE_PROFILES=db,share` in the settings file and run `up -d` without `--profile` (section 11). |
| Phone sign-in says the number is invalid | The number doesn't match the country | Australia: mobile like `412 345 678`. India: `98765 43210` (starts with 6–9, 10 digits). |

Still stuck? Collect this and share it with the team:

```sh
docker version
docker compose version
docker compose --env-file au.env ps -a
docker compose --env-file au.env logs --tail 200 migrate api worker web > feast-logs.txt
```

Check `feast-logs.txt` and remove any secrets before sending it.

---

## 16. Safety notes

- **Development mode is for testing only.** Payments are simulated, the
  sign-in code is always `123456`, and social login accepts any name. Don't
  put real customers on it.
- **Never commit `au.env` or `in.env`.** They are in `.gitignore`. Only the
  `*.env.example` files belong in Git.
- Before using the share link (section 11), change `SEED_PASSWORD` from
  anything guessable.
- Going live with real payments, SMS and push notifications is covered in
  [`deploy/compose/README.md`](deploy/compose/README.md) (single machine) and
  [`deploy/README.md`](deploy/README.md) (AWS, Azure, Google Cloud).

---

### Where to go next

| You want to…                     | Read                                                                 |
|----------------------------------|----------------------------------------------------------------------|
| Understand the code layout       | [`README.md`](README.md), [`backend/README.md`](backend/README.md)   |
| Develop the API without Docker   | `cd backend/rust && FEAST_COUNTRY=in cargo run`                      |
| Develop the web app              | `cd webapp && npm install && FEAST_API_INTERNAL_URL=http://localhost:8080 npm run dev` |
| Deploy to a cloud                | [`deploy/README.md`](deploy/README.md)                               |
| See the API                      | [`backend/spec/openapi.yaml`](backend/spec/openapi.yaml)             |
