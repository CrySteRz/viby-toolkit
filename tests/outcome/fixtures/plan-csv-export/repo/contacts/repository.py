"""In-memory storage with the same interface as the SQL repository used in production."""
import itertools


class ContactRepository:
    def __init__(self):
        self._rows = {}
        self._ids = itertools.count(1)

    def next_id(self):
        return next(self._ids)

    def add(self, contact):
        self._rows[contact.id] = contact
        return contact

    def get(self, contact_id):
        return self._rows.get(contact_id)

    def save(self, contact):
        self._rows[contact.id] = contact
        return contact

    def list(self, include_archived=False):
        rows = [self._rows[i] for i in sorted(self._rows)]
        return rows if include_archived else [c for c in rows if not c.archived]

    def find_by_email(self, email):
        email = email.lower()
        return next((c for c in self._rows.values() if c.email.lower() == email), None)
