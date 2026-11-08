"""Business rules for notes. The clock is injectable so tests control time."""
from datetime import datetime, timezone

from .errors import NotFound, ValidationError
from .models import Note
from .repository import NoteRepository

MAX_TITLE = 120


def _utc_now():
    return datetime.now(timezone.utc)


class NoteService:
    def __init__(self, repo=None, clock=None):
        self.repo = repo or NoteRepository()
        self.clock = clock or _utc_now

    def _validate(self, title, tags):
        if not title or not title.strip():
            raise ValidationError("title is required")
        if len(title) > MAX_TITLE:
            raise ValidationError(f"title is longer than {MAX_TITLE} characters")
        if any(not isinstance(t, str) or not t.strip() for t in tags):
            raise ValidationError("tags must be non-empty strings")

    def create(self, title, body="", tags=()):
        tags = list(tags)
        self._validate(title, tags)
        now = self.clock()
        note = Note(self.repo.next_id(), title.strip(), body, now, now, tags)
        return self.repo.add(note)

    def get(self, note_id):
        note = self.repo.get(note_id)
        if note is None:
            raise NotFound(f"note {note_id} not found")
        return note

    def update(self, note_id, title=None, body=None, tags=None):
        note = self.get(note_id)
        new_title = note.title if title is None else title
        new_tags = note.tags if tags is None else list(tags)
        self._validate(new_title, new_tags)
        note.title, note.tags = new_title.strip(), new_tags
        if body is not None:
            note.body = body
        note.updated_at = self.clock()
        return self.repo.save(note)

    def delete(self, note_id):
        if not self.repo.remove(note_id):
            raise NotFound(f"note {note_id} not found")

    def list(self, tag=None):
        notes = self.repo.list()
        return [n for n in notes if tag in n.tags] if tag else notes

    def search(self, text):
        return self.repo.search(text)
