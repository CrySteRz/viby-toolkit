from dataclasses import dataclass, field


@dataclass
class Note:
    id: int
    title: str
    body: str
    created_at: object  # aware datetime
    updated_at: object
    tags: list = field(default_factory=list)

    def to_dict(self):
        return {
            "id": self.id,
            "title": self.title,
            "body": self.body,
            "tags": list(self.tags),
            "created_at": self.created_at.isoformat(),
            "updated_at": self.updated_at.isoformat(),
        }
