# Deploying koryto on the home server

Everything here runs on the host, not in a sandbox. Steps are in order; each
one can be checked before the next. `PLAN.md` explains the design; this is
only the runbook.

## 1. authentik

1. Optional: Directory → Groups: create `koryto`, add both of you, and
   either bind it to the application as a policy (step 3) or set
   `KORYTO_OIDC_GROUP=koryto`. Without either, any authentik account may log
   in; the household is what gates the data.
2. Applications → Providers → Create → OAuth2/OpenID Provider:
   - Authorization flow: the default explicit consent (or implicit) flow
   - Client type: Confidential
   - Redirect URIs: `https://koryto.int.krzaq.cc/api/auth/callback` (strict)
   - Signing key: any RSA key
   - Scopes: `openid`, `email`, `profile` (the default profile mapping
     already includes `groups`)
3. Applications → Create: name `koryto`, slug `koryto`, bind the provider.
   To gate the login in authentik, add a policy binding for the group here.
4. Note the issuer `https://authentik.krzaq.cc/application/o/koryto/`, the
   client id and the client secret.

## 2. DNS and Apache

Add `koryto.int.krzaq.cc` to the internal zone the way the other
`*.int.krzaq.cc` names are done (see the notes in `/etc/named.conf`). The
wildcard certificate already covers it. Then:

```sh
sudo install -m 644 packaging/httpd/koryto.int.krzaq.cc /etc/httpd/conf/vhosts/
sudo /root/scripts/vhost add koryto.int.krzaq.cc
sudo apachectl configtest && sudo systemctl reload httpd
```

The vhost proxies to `127.0.0.1:13384`; the header comment says why it has no
CORS, no `/api` bypass and no websocket rewrite.

## 3. Deploy

Two ways to run the same `docker-compose.yml`.

**On this host**, the run directory is its own ZFS dataset,
`storage/encrypted/koryto` at `/storage/encrypted/koryto`, holding only the
compose file, `.env` and `data/postgres`. The image is built in the
development checkout and started there; nothing in the run directory is a
git checkout.

```sh
sudo zfs create -o compression=zstd storage/encrypted/koryto
sudo chown krzaq:krzaq /storage/encrypted/koryto
cp .env.example /storage/encrypted/koryto/.env   # fill POSTGRES_PASSWORD, KORYTO_SECRET, the OIDC values
cp Makefile.local.example Makefile.local          # RUN_DIR is already that path
make deploy                                       # builds koryto:local here, copies the compose file, starts the stack there
cd /storage/encrypted/koryto && docker compose logs -f koryto   # wait for "listening on"
```

`make image` also tags the build with the commit (`koryto:<sha>`), so an
older image is one `docker tag koryto:<sha> koryto:local` away if a deploy
goes wrong.

**Anywhere else**, a clone is enough: the compose file carries `build: .`,
so `docker compose up -d --build` in the checkout builds and runs the stack
in place, with `.env` and `data/` next to it.

Migrations run on first start. `./data/postgres` ends up owned by uid 999,
the image's postgres user. DataGrip reaches the database at
`127.0.0.1:13385`.

## 4. Household

Both of you log in once through the browser at
`https://koryto.int.krzaq.cc`. Each login gets a household of its own, so
the app works alone from the first day. Sharing is a merge, on the host:

```sh
cd /storage/encrypted/koryto
docker compose exec koryto koryto household add-member <her email> --to <your email>
docker compose exec koryto koryto household rename <your email> home
docker compose exec koryto koryto household list
```

The joiner's entries come along by construction; their saved foods move
where the name is free and merge where it is taken. `remove-member <email>`
is the reverse: back to a household of their own with a copy of the foods.

On the Profile page set each person's height, birth date, sex and activity
factor (they only feed the seed of the expenditure estimate), and a first
target. Weights and meals can start before that.

## 5. Tokens

On the Tokens page (session only; tokens cannot make tokens):

- `openwebui`: scopes `read,write,edit`, tick **delegate**. This is the
  gateway token: every request must carry `X-Koryto-User: <email>` naming
  the acting person, who must have logged in here before.
- `claude-code`: scopes `read,write,edit`, acting as you.

The gateway token acts only for people who have logged into koryto through
the browser in the last 30 days, so removing someone in authentik stops
their Open WebUI access within that time on its own. To stop someone seeing
the shared log at once, `koryto household remove-member <email>`: they keep
their own entries, in a household of their own, and lose sight of yours.

Register `https://koryto.int.krzaq.cc/mcp` in Open WebUI with the bearer
token and the `X-Koryto-User` header set from the acting user, the same way
the support server's `X-Support-User` is wired. In Claude Code:

```sh
claude mcp add --transport http koryto https://koryto.int.krzaq.cc/mcp \
  --header "Authorization: Bearer ko_..."
```

## 6. Backups

The existing backup job covers MariaDB, not this container. Add

```sh
docker compose exec -T db pg_dump -U koryto koryto
```

to it, or include `./data/` in a file-level backup taken while the stack is
stopped (a live copy of `./data/postgres` is not a consistent backup).

## 7. Acceptance

1. From Open WebUI, as each of you: "I had two eggs on toast" — the model
   should search foods, estimate, ask, and log on `confirmed=true`. Check
   the Day page shows it for the right person.
2. "Lentil curry for both of us, I had a double portion" after saving the
   food once — two entries, one per person.
3. Log a weight, then "I'm in New York now": `set_location`, and a late
   dinner logged afterwards lands on the American day while the earlier
   entries did not move. The Day page badge says "on New York time".
4. Open the Trends page in light and dark theme and look at the charts:
   the label collisions and overflow the sandbox could not see are checked
   here.

## Updating

From the development checkout, on the commit to ship:

```sh
make deploy
cd /storage/encrypted/koryto && docker compose logs -f koryto
```

Migrations apply at startup. `koryto recompute-days` exists for the day a
rule about days changes; nothing routine needs it.
