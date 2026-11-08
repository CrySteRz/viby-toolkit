"""HTTP-shaped handler: handle(method, path, body) -> (status, json-ready body)."""
import re

from .subscriptions import SubscriptionStore

SUB = re.compile(r"^/subscriptions/(\d+)$")


class Api:
    def __init__(self, sender, store=None):
        self.sender = sender
        self.store = store or SubscriptionStore()
        self.log = []

    def handle(self, method, path, body=None):
        body = body or {}
        if path == "/subscriptions" and method == "POST":
            try:
                sub = self.store.add(body.get("url", ""), body.get("events", ()), body.get("secret", ""))
            except ValueError as e:
                return 400, {"error": str(e)}
            return 201, {"id": sub.id}
        m = SUB.match(path)
        if m and method == "DELETE":
            return (204, None) if self.store.remove(int(m.group(1))) else (404, {"error": "not found"})
        if path == "/events" and method == "POST":
            if not body.get("type"):
                return 400, {"error": "type is required"}
            results = [self.sender.send(s, body) for s in self.store.for_event(body["type"])]
            self.log.extend(results)
            deliveries = [{"subscription": r.subscription_id, "ok": r.ok, "status": r.status} for r in results]
            return (202 if all(r.ok for r in results) else 502), {"deliveries": deliveries}
        return 404, {"error": "no such route"}
