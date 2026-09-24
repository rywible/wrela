/** Bridge for fresh agents launched by a host instead of a command-line model SDK. */
if (import.meta.main) {
  const request = await Bun.file(process.argv[2]).json();
  while (!(await Bun.file(request.submission).exists())) await Bun.sleep(250);
  // Authors publish submission.json atomically as their final action.
}
