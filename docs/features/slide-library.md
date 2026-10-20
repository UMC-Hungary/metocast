# Slide library search

The slide library is every PowerPoint deck in the folders the server watches. Search finds a deck
by its **title**, its **hymn number** or **words from its slides**, and lists the best match first.

- **Hymn number**: `47` finds _Ó, hála az Úrnak (47)_. The number is the last one in parentheses
  in the file name. Typing `4` lists 4 first, then 40–49 and 400 and up, then decks whose text
  has a word starting with 4.
- **Title**: `hala urnak` finds _Ó, hála az Úrnak (47)_. Accents and case don't matter, and each
  word matches as the start of a word, so `hal` finds _hála_ and _hálát_.
- **Lyrics**: `pajzs` finds the deck whose slides say _pajzsunk_, with a line of the slide text
  around the match.

Results are ranked by why they matched: hymn number first, then title, then slide text. Each
result shows what matched: the matched words are highlighted in the title, and a lyric match
also shows the slide text around it. An empty search lists every deck by title.

Only `.pptx` slide text can be read. A legacy `.ppt` deck is found by its title and number only.

## The index

The server keeps the searchable text in its database (`ppt_decks`), one row per deck.

- **Searching keeps it fresh.** Each search first checks the watched folders. A deck whose file is
  new or changed (by modification time and size) is read again, and a deleted deck is dropped.
  Unchanged decks are not read.
- **Adding a folder indexes it in the background**, so its decks are ready for the first search.
  While decks are being read, every app shows an indexing icon in its header. Its tooltip says
  how many decks are being read. A search during that time answers from what is already indexed.
  The open list searches again when indexing ends.
- **Removing a folder removes its decks from the index.**
- **A folder that can't be read keeps its decks**, for example an external drive that is asleep
  or unplugged. They are dropped only once the folder can be read and the files are gone.

The server log records every index run that changed something (decks added, updated, removed and
unreadable, and how long it took). It also records a deck that can't be read, once per version of
the file, and a folder that becomes unreadable or readable again.

## Use it

**Apple app**

- **Presentation** and the Order of Service's **Song…** picker share the library list.
- Type in the search field at the top of the list (on the iPhone, the search bar).
- The folder menu in the toolbar picks a folder and adds or removes watched folders.
- A row shows the title with the matched words highlighted, the hymn number on the right, and
  the matching slide text when the lyrics matched.
- While the server indexes, a pulsing magnifier shows in the toolbar beside the bell. On the Mac,
  hover over it for the deck count; on the iPhone, tap it.

**Sanctum**

- **Presentations** and the Order of Service's song picker search the same way, with the folder
  select beside the search field.
- While the server indexes, a spinner shows beside the bell in the page header, with a tooltip.

**Companion**

The PPT selector's search uses the same ranking, so typing a hymn number on the keypad puts that
hymn in the first slot.

## Entry points

| Does                   | REST                                          | WebSocket                                                            | MCP tool             |
| ---------------------- | --------------------------------------------- | -------------------------------------------------------------------- | -------------------- |
| Search decks           | `GET /api/ppt/files?filter=&folderId=&limit=` | `ppt.search` (`filter`, `folder_id`, `limit`) → `ppt.search_results` | `ppt.search`         |
| List watched folders   | `GET /api/ppt/folders`                        | `ppt.folders.list`                                                   | `ppt.folders.list`   |
| Watch a folder         | `POST /api/ppt/folders`                       | `ppt.folders.add`                                                    | `ppt.folders.add`    |
| Stop watching a folder | `DELETE /api/ppt/folders/{id}`                | `ppt.folders.remove`                                                 | `ppt.folders.remove` |

- `limit` is 5 when omitted (Companion's slots) and at most 200. The apps ask for 100.
- Each result has `id`, `name`, `path`, `folderId`, `title`, `number` (or null) and `matchedIn`
  (`number`, `title`, `text`, or null for an empty search). `titleParts` and `snippetParts` are
  lists of `{ text, hit }`, with `hit` marking the matched words. `snippetParts` is empty unless
  the slide text matched.
- `ppt.search_results` goes to every connected client, as before.

Pushes:

- `ppt.index.state` `{ indexing, pending }` is sent when the server starts reading decks
  (`pending` is how many) and when it finishes (`pending` is 0). It is also sent on connect.
- `ppt.folders_changed` is sent when a folder is added or removed.
