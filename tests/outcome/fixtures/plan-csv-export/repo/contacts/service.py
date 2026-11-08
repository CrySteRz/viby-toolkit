"""Business rules for contacts."""
import re
from datetime import datetime, timezone

from .errors import NotFound, ValidationError
from .models import Contact
from .repository import ContactRepository

EMAIL = re.compile(r"^[^@\s]+@[^@\s]+\.[^@\s]+$")


class ContactService:
    def __init__(self, repo=None, clock=None):
        self.repo = repo or ContactRepository()
        self.clock = clock or (lambda: datetime.now(timezone.utc))

    def create(self, name, email, company="", tags=()):
        name, email = (name or "").strip(), (email or "").strip()
        if not name:
            raise ValidationError("name is required")
        if not EMAIL.match(email):
            raise ValidationError(f"not an e-mail address: {email!r}")
        if self.repo.find_by_email(email):
            raise ValidationError(f"{email} already exists")
        contact = Contact(self.repo.next_id(), name, email, company or "", self.clock(), list(tags))
        return self.repo.add(contact)

    def get(self, contact_id):
        contact = self.repo.get(contact_id)
        if contact is None:
            raise NotFound(f"contact {contact_id} not found")
        return contact

    def archive(self, contact_id):
        contact = self.get(contact_id)
        contact.archived = True
        return self.repo.save(contact)

    def list(self, include_archived=False, tag=None):
        rows = self.repo.list(include_archived=include_archived)
        return [c for c in rows if tag in c.tags] if tag else rows
