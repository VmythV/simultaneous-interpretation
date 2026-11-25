#!/usr/bin/env bash
# 创建本地自签名的代码签名证书「TingYi Local Signing」，打包时用它给听译签名。
#
# 好处：重新打包后 macOS 仍认为是同一个 App，麦克风 / 系统录音权限不用每次重新授权。
# 只需运行一次。过程中 macOS 会弹窗要求输入登录密码（修改证书信任设置）。
# 删除：钥匙串访问 → 登录 → 我的证书 → 删除「TingYi Local Signing」。
set -euo pipefail

NAME="TingYi Local Signing"
KEYCHAIN="$HOME/Library/Keychains/login.keychain-db"

if security find-certificate -c "$NAME" "$KEYCHAIN" >/dev/null 2>&1; then
  echo "证书已存在：$NAME"
  exit 0
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

cat > "$TMP/cert.cnf" <<EOF
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = $NAME
[ext]
basicConstraints = critical,CA:false
keyUsage = critical,digitalSignature
extendedKeyUsage = critical,codeSigning
EOF

# 用系统自带的 LibreSSL，生成的 p12 能被钥匙串直接导入
OPENSSL=/usr/bin/openssl
PASS="tingyi-$$"
$OPENSSL req -x509 -newkey rsa:2048 -nodes -days 3650 \
  -keyout "$TMP/key.pem" -out "$TMP/cert.pem" -config "$TMP/cert.cnf" 2>/dev/null
$OPENSSL pkcs12 -export -inkey "$TMP/key.pem" -in "$TMP/cert.pem" \
  -out "$TMP/cert.p12" -passout "pass:$PASS" -name "$NAME"

# 导入登录钥匙串，并允许 codesign 使用这把私钥
security import "$TMP/cert.p12" -k "$KEYCHAIN" -P "$PASS" -T /usr/bin/codesign
# 信任为代码签名证书（会弹窗要求输入登录密码）
security add-trusted-cert -r trustRoot -p codeSign -k "$KEYCHAIN" "$TMP/cert.pem"

echo
echo "已创建证书：$NAME"
security find-identity -p codesigning "$KEYCHAIN" | grep "$NAME" || true
echo "之后运行 npm run app:build 会自动使用这个证书签名。"
echo "第一次签名时如果弹出「codesign 想要使用钥匙串中的密钥」，请点「始终允许」。"
