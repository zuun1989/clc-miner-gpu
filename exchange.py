from ecdsa import SigningKey, SECP256k1
import hashlib
import binascii
import requests
import time

EXCHANGE_COIN_ID = 248

balanceCLC: dict[str, float] = {}

topupExchnagePrivateKeys: dict[str, str] = {}
topupTXIds: dict[str, int] = {}

# @route("/clc/topup/:txid")
def topupCLC(txid: int, user_id: str) -> str:
    if topupExchnagePrivateKeys.get(user_id): return "Already invoicing!"
    topupTXIds[user_id] = txid
    kp = SigningKey.generate(curve=SECP256k1)
    topupExchnagePrivateKeys[user_id] = kp.to_string().hex()
    return f"Please transact as much clc as you wish to coin #{txid}, address: {kp.get_verifying_key().to_string().hex()}, once done, please press refresh"

# @route("/clc/topup/refresh")
def refresh(user_id: str):
    if not topupTXIds.get(user_id): return "Not yet invoicing!"
    kp = SigningKey.from_string(binascii.unhexlify(topupExchnagePrivateKeys[user_id]), curve=SECP256k1)
    coin = requests.get("https://clc.ix.tc/coin/" + str(topupTXIds[user_id])).json()["coin"]
    if coin["transactions"][-1]["holder"] == kp.get_verifying_key().to_string().hex():
        if not balanceCLC.get(user_id): balanceCLC[user_id] = 0
        balanceCLC[user_id] += coin["val"]
        
        exchange_coin = requests.get("https://clc.ix.tc/coin/" + str(EXCHANGE_COIN_ID)).json()["coin"]
        sign = kp.sign_digest(hashlib.sha256(f"{EXCHANGE_COIN_ID} {len(exchange_coin["transactions"])} {coin["val"]}").digest()).hex()
        res = requests.get(f"https://clc.ix.tc/merge?origin={topupTXIds[user_id]}&target={EXCHANGE_COIN_ID}&sign={sign}&vol={coin["vol"]}").json()

        if res.get("error"): return "Error merging into wallet: " + res["error"]

        del topupExchnagePrivateKeys[user_id]
        del topupTXIds[user_id]
        return f"Added {round(coin["val"], 2)}CLC to your balance"

    return "Haven't received any CLC yet, try again in a few seconds..."
    

print(topupCLC(16585, "john_user_secret")) # User provides these, generated in clc wallet. `john_user_secret` is platform account, no idea how you handle it.

while True:
    time.sleep(3)
    print(refresh("john_user_secret"))