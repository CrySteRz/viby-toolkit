"""An in-memory inverted index over document titles and bodies."""
import re

WORD = re.compile(r"[a-z0-9]+")


def tokens(text):
    return WORD.findall(text.lower())


class Index:
    def __init__(self):
        self._docs = {}
        self._postings = {}

    def add(self, doc_id, title, body=""):
        if doc_id in self._docs:
            self.remove(doc_id)
        self._docs[doc_id] = {"id": doc_id, "title": title, "body": body}
        for tok in set(tokens(title) + tokens(body)):
            self._postings.setdefault(tok, set()).add(doc_id)

    def remove(self, doc_id):
        doc = self._docs.pop(doc_id)
        for tok in set(tokens(doc["title"]) + tokens(doc["body"])):
            self._postings[tok].discard(doc_id)

    def get(self, doc_id):
        return self._docs[doc_id]

    def query(self, text):
        """Documents containing every word of `text`, ordered by id."""
        words = tokens(text)
        if not words:
            return []
        ids = set.intersection(*(self._postings.get(w, set()) for w in words))
        return [self._docs[i] for i in sorted(ids)]
