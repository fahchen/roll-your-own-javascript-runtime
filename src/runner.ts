(async () => {
  const query = await import("jet:query");
  return query.handle({ to: "Alice" }, { current_user: { name: "Alice" } });
})();
