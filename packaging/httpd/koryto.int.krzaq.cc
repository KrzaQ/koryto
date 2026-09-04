# Apache vhost for koryto.int.krzaq.cc — reverse proxy to the koryto
# container on 13384.
#
# Install with:
#   sudo install -m 644 packaging/httpd/koryto.int.krzaq.cc /etc/httpd/conf/vhosts/
#   sudo /root/scripts/vhost add koryto.int.krzaq.cc
#   sudo apachectl configtest && sudo systemctl reload httpd
#
# Structure follows support.int.krzaq.cc. Deliberate choices:
#
#   * No Access-Control-Allow-Origin. The Vue app is same-origin and the API
#     authenticates with a cookie, so a wildcard ACAO would let any page a
#     browser visits read and edit the household's log with the user's session.
#   * No /api bypass. Bearer tokens for MCP clients travel in the Authorization
#     header through the same proxy; nothing here needs to be unauthenticated.
#   * No websocket handling. The app polls nothing and streams nothing; MCP
#     uses the streamable HTTP transport, which is plain POST/GET.
#
# There is no DocumentRoot on the *:80 vhost: int.krzaq.cc gets its wildcard
# certificate through DNS-01, so nothing needs to serve an HTTP-01 challenge.
# See the notes in /etc/named.conf.
#
# Whether the name resolves from the internet as well as the LAN is decided in
# the DNS views, not here. Nothing here limits by source address: the app does
# its own OIDC login and group check, and /mcp accepts bearer tokens only.

<VirtualHost *:80>
	ServerName koryto.int.krzaq.cc

	CustomLog "/home/krzaq/logs/krzaq.cc" common
	ErrorLog "/home/krzaq/logs/krzaq.cc.error"

	Redirect permanent / https://koryto.int.krzaq.cc/
</VirtualHost>

<VirtualHost *:443>
	ServerName koryto.int.krzaq.cc

	CustomLog "/home/krzaq/logs/krzaq.cc" common
	ErrorLog "/home/krzaq/logs/krzaq.cc.error"

	SSLEngine on
	# Wildcard from DNS-01. Verify the lineage name with `certbot certificates`
	# before reloading — certbot strips the `*.`, so this should be
	# int.krzaq.cc, but a second issuance can land in int.krzaq.cc-0001 and
	# Apache will refuse to start on a missing file.
	SSLCertificateFile "/etc/letsencrypt/live/int.krzaq.cc/fullchain.pem"
	SSLCertificateKeyFile "/etc/letsencrypt/live/int.krzaq.cc/privkey.pem"

	# The app builds its OIDC redirect URI from KORYTO_PUBLIC_URL, not from
	# request headers, so this is informational; kept for parity with the
	# other OIDC vhosts and for anything that logs the scheme.
	RequestHeader set X-Forwarded-Proto "https"

	# Reverse proxy only; never a forward proxy.
	ProxyRequests Off
	<Proxy *>
		Require all granted
	</Proxy>

	# Every request is a quick database round trip; the default ProxyTimeout
	# is ample.
	ProxyPass        / http://127.0.0.1:13384/
	ProxyPassReverse / http://127.0.0.1:13384/
	ProxyPreserveHost On
</VirtualHost>
