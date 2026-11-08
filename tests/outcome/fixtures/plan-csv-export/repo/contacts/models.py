from dataclasses import dataclass, field


@dataclass
class Contact:
    id: int
    name: str
    email: str
    company: str
    created_at: object  # aware datetime
    tags: list = field(default_factory=list)
    archived: bool = False

    def to_dict(self):
        return {
            "id": self.id,
            "name": self.name,
            "email": self.email,
            "company": self.company,
            "tags": list(self.tags),
            "created_at": self.created_at.isoformat(),
            "archived": self.archived,
        }
