"""Storage. In memory here; production swaps in the SQL repository with the same methods."""
import itertools


class NoteRepository:
    def __init__(self):
        self._notes = {}
        self._ids = itertools.count(1)

    def next_id(self):
        return next(self._ids)

    def add(self, note):
        self._notes[note.id] = note
        return note

    def get(self, note_id):
        """The note, or None."""
        return self._notes.get(note_id)

    def save(self, note):
        self._notes[note.id] = note
        return note

    def remove(self, note_id):
        return self._notes.pop(note_id, None) is not None

    def list(self):
        return [self._notes[i] for i in sorted(self._notes)]

    def search(self, text):
        needle = text.lower()
        return [n for n in self.list() if needle in n.title.lower() or needle in n.body.lower()]
