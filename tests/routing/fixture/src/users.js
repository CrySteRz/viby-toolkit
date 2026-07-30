const cache = new Map();
export async function getUser(id) {
  if (cache.has(id)) return cache.get(id);
  const row = await db.query(`SELECT * FROM users WHERE id = '${id}'`);
  cache.set(id, row);
  return row;
}
