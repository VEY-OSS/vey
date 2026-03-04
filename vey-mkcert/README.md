# VEY MkCert

A tool to generate certificates, including:

- root CA
- intermediate CA
- TLS server certificate
- TLS client certificate
- TLCP server sign certificate
- TLCP server encrypt certificate
- TLCP client sign certificate
- TLCP client encrypt certificate

## How to build

### Use default installed OpenSSL

```shell
cargo build -p vey-mkcert
```

### Use latest OpenSSL

```shell
cargo build -p vey-mkcert --features vendored-openssl
```

### Use Tongsuo

```shell
cargo build -p vey-mkcert --features vendored-tongsuo
```

## How to use

### Generate a root CA certificate

```shell
vey-mkcert --root --common-name "VEY test ROOT CA" --rsa 2048 --output-cert rootCA.crt --output-key rootCA.key
```

### Generate a TLS certificates

server side:

```shell
vey-mkcert --tls-server --ec256 --common-name "Example Server" --host www.example.net --ca-cert rootCA.crt --ca-key rootCA.key
```

client side:

```shell
vey-mkcert --tls-client --ec256 --common-name "Example Client" --host www.example.net --ca-cert rootCA.crt --ca-key rootCA.key
```

### Generate TLCP certificates

server side:

```shell
vey-mkcert --tlcp-server-sign --sm2 --common-name "Example Server Sign" --host www.example.net --ca-cert rootCA.crt --ca-key rootCA.key
vey-mkcert --tlcp-server-enc --sm2 --common-name "Example Server Enc" --host www.example.net --ca-cert rootCA.crt --ca-key rootCA.key
```

client side:

```shell
vey-mkcert --tlcp-client-sign --sm2 --common-name "Example Client Sign" --host www.example.net --ca-cert rootCA.crt --ca-key rootCA.key
vey-mkcert --tlcp-client-enc --sm2 --common-name "Example Client Enc" --host www.example.net --ca-cert rootCA.crt --ca-key rootCA.key
```

### Generate a mimic certificate

```shell
vey-mkcert --mimic input.crt --ca-cert rootCA.crt -ca-key rootCA.key --output-cert mimic.crt --output-key mimic.key
```
