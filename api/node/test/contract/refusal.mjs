// A LayerFSError as the library answers the refusal (call surface 2.0), so a
// test can compare the package with the command's one JSON line on stderr:
// `{ error, message }`, `fix` when there is one, and the fields the next
// call needs, flat. The long form (`long`: the call asked `detail`) always
// has `fix`, and adds `detail`.
export function refusalOf(error, long = false) {
  // `message` and `stack` are not enumerable; what is left over is the fields
  const { name: _name, code: _code, error: word, fix, detail, ...fields } = error;
  return {
    error: word,
    message: error.message,
    ...(long || fix !== null ? { fix } : {}),
    ...fields,
    ...(long ? { detail } : {}),
  };
}
