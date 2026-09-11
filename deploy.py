#!/usr/bin/env python3
"""Zero-argument build/deployer for both independent bridge programs.

Edit RPC_URL and PRIVATE_KEY_BASE58, then run: python3 deploy.py
Do not commit or share the populated file.
"""
import json, os, re, stat, subprocess, tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent
RPC_URL = os.getenv("SOLANA_RPC_URL", "https://api.mainnet-beta.solana.com")
PRIVATE_KEY_BASE58 = os.getenv("SOLANA_PRIVATE_KEY", "PASTE_BASE58_PRIVATE_KEY_HERE")
BASE58_ALPHABET = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"

def decode_base58(value: str) -> bytes:
    number = 0
    try:
        for character in value:
            number = number * 58 + BASE58_ALPHABET.index(character)
    except ValueError:
        raise SystemExit("PRIVATE_KEY_BASE58 contains a character that is not valid Base58.")
    decoded = number.to_bytes((number.bit_length() + 7) // 8, "big") if number else b""
    return b"\0" * (len(value) - len(value.lstrip("1"))) + decoded

def run(*args: str) -> str:
    p = subprocess.run(args, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    if p.returncode: raise SystemExit(p.stdout)
    return p.stdout

def main() -> None:
    secret = decode_base58(PRIVATE_KEY_BASE58)
    if len(secret) != 64:
        raise SystemExit(f"PRIVATE_KEY_BASE58 must decode to a 64-byte Solana keypair; got {len(secret)} bytes.")
    deploy = ROOT / "target/deploy"; deploy.mkdir(parents=True, exist_ok=True)
    programs = [("independent_burn_bridge", "Brdg111111111111111111111111111111111111111"),
                ("message_transmitter", "MsgT111111111111111111111111111111111111111")]
    ids = {}
    for name, placeholder in programs:
        kp = deploy / f"{name}-keypair.json"
        if not kp.exists(): run("solana-keygen", "new", "--no-bip39-passphrase", "--silent", "-o", str(kp))
        ids[name] = run("solana-keygen", "pubkey", str(kp)).strip()
        for f in (ROOT / "Anchor.toml", ROOT / f"programs/{name}/src/lib.rs"):
            f.write_text(f.read_text().replace(placeholder, ids[name]))
    run("anchor", "build")
    signatures = {}
    with tempfile.TemporaryDirectory(prefix="bridge-deployer-") as tmp:
        payer = Path(tmp) / "payer.json"
        payer.write_text(json.dumps(list(secret)))
        payer.chmod(stat.S_IRUSR | stat.S_IWUSR)
        for name, _ in reversed(programs):
            out = run("solana", "program", "deploy", "--url", RPC_URL, "--keypair", str(payer),
                      "--program-id", str(deploy / f"{name}-keypair.json"), str(deploy / f"{name}.so"))
            match = re.search(r"Signature: (\S+)", out)
            signatures[name] = match.group(1) if match else "see CLI output"
    print(json.dumps({"rpc": RPC_URL, "program_ids": ids, "deployment_signatures": signatures}, indent=2))

if __name__ == "__main__": main()
