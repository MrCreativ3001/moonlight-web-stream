from OpenSSL import crypto, SSL
from socket import gethostname
from pprint import pprint
from time import gmtime, mktime
import os

def create_self_signed_cert(key_path, cert_path):
         
    # create a key pair
    k = crypto.PKey()
    k.generate_key(crypto.TYPE_RSA, 2048)

    # create a self-signed cert
    cert = crypto.X509()
    cert.get_subject().C = "UK"
    cert.get_subject().ST = "London"
    cert.get_subject().L = "London"
    cert.get_subject().O = "Dummy Company Ltd"
    cert.get_subject().OU = "Dummy Company Ltd"
    cert.get_subject().CN = gethostname()
    cert.set_serial_number(1000)
    cert.gmtime_adj_notBefore(0)
    cert.gmtime_adj_notAfter(10*365*24*60*60)
    cert.set_issuer(cert.get_subject())
    cert.set_pubkey(k)
    cert.sign(k, 'sha1')

    open(cert_path, "wb").write(crypto.dump_certificate(crypto.FILETYPE_PEM, cert))
    open(key_path, "wb").write(crypto.dump_privatekey(crypto.FILETYPE_PEM, k))

os.makedirs("./server", exist_ok=True)

create_self_signed_cert("./server/key.pem", "./server/cert.pem")