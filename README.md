# trial-proxy-wasm

## What

Proxy Wasm trial.

## How to run

Prepare to run envoy

```
$ rustup target add wasm32-unknown-unknown
$ cargo build --target=wasm32-unknown-unknown --release
$ docker-compose up -d
$ docker-compose ps
          Name                      Command             State             Ports
------------------------------------------------------------------------------------------
trial-proxy-wasm_envoy_1   /docker-entrypoint.sh envo   Up      10000/tcp,
                           ...                                  0.0.0.0:8000->8000/tcp,
                                                                0.0.0.0:9000->9000/tcp
trial-proxy-wasm_nginx_1   /docker-entrypoint.sh ngin   Up      80/tcp
```

User's request will be restricted by static rule allows 3 requests for 20 sec.

X-User-ID header has a string describes user's ID.

```
$ curl -s -o /dev/null -w '%{http_code}' -H 'x-user-id: 1' http://localhost:8000/
200
$ curl -s -o /dev/null -w '%{http_code}' -H 'x-user-id: 1' http://localhost:8000/
200
$ curl -s -o /dev/null -w '%{http_code}' -H 'x-user-id: 1' http://localhost:8000/
200
$ curl -s -o /dev/null -w '%{http_code}' -H 'x-user-id: 1' http://localhost:8000/
429
$ curl -s -o /dev/null -w '%{http_code}' -H 'x-user-id: 1' http://localhost:8000/
429
$ curl -s -o /dev/null -w '%{http_code}' -H 'x-user-id: 1' http://localhost:8000/
429
$
...
```