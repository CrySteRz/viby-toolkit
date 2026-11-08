"""Short codes: base62 of a sequence number, so codes are short and never collide."""
import string

ALPHABET = string.digits + string.ascii_letters


def encode(n):
    if n < 0:
        raise ValueError("n must be non-negative")
    if n == 0:
        return ALPHABET[0]
    out = []
    while n:
        n, r = divmod(n, 62)
        out.append(ALPHABET[r])
    return "".join(reversed(out))


def decode(code):
    n = 0
    for ch in code:
        n = n * 62 + ALPHABET.index(ch)
    return n
