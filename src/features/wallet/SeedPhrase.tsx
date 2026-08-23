/**
 * A seed phrase, numbered, in a fixed grid.
 *
 * Numbered because order is part of the secret — a phrase transcribed correctly but out of
 * order restores nothing. Deliberately not selectable-and-copyable by default: the clipboard is
 * read by every other program on the machine, and a phrase that lands there is a phrase that
 * has left the credential store.
 */
export function SeedPhrase({ phrase }: { phrase: string }) {
  const words = phrase.trim().split(/\s+/).filter(Boolean);

  return (
    <ol className="seed-phrase">
      {words.map((word, i) => (
        <li className="seed-phrase__word" key={`${i}-${word}`}>
          <span className="seed-phrase__index mono-nums">{i + 1}</span>
          <span className="seed-phrase__text">{word}</span>
        </li>
      ))}
    </ol>
  );
}
