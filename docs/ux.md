# Designing screens that work

Use this when you create a screen, page or layout (not a component set): apply it to every screen you build and to what you change. The numbers are web defaults;
for a mobile screen the body text is 16px and touch targets are 44px.

## Three levels of content: Obvious, Easy, Possible

Every screen has only these, in this order of attention:

1. **Obvious**: what the person must see first and what the screen is about: the picture, the video, the big headline, the key number. One per screen (or per section).
2. **Easy**: what makes the Obvious easy to take in: the title under the picture, the subtitle under the headline, a short description, a label.
3. **Possible**: what they can do after reading the two above: buttons, links, inputs.

If a layer does not belong to one of the three, remove it. Do not put the Possible before the Obvious, and do not give two things the Obvious treatment.

## Hierarchy

- Size, weight, color and space decide what is read first. Compare every pair of neighbours: one must clearly win.
- Hierarchy also lives inside an element: three buttons are one primary (filled, accent), one secondary (outline or soft fill) and one tertiary (text only). Never two primaries side by side.
- Order the page by importance, top to bottom, left to right; the first thing in the reading order is the Obvious.
- Few levels: one display or heading size, one subheading, body, helper. A fifth size is a sign something is wrong.

## Consistency

Same thing, same look, everywhere on every screen of the file.

- **Space** comes from one scale in steps of 4: 4, 8, 12, 16, 24, 32, 48, 64. Use the same page margin on every screen and the same gap between like things.
  Set it with auto layout (`gap`, `pad`), not by eye.
- Reuse the same radius, the same stroke width, the same icon set and size, the same input and button heights.
- Reuse the colors and text styles of the file (`GET /styles`, `GET /variables`) before you make new ones. Make a new one only when nothing fits, and name it.
- Align to a grid: left edges line up, gutters are equal, centered things are centered on the same axis.

## Readability and legibility

- Body text is at least 14px on web and 16px on mobile. Helper text (captions, labels) may be 12px; an UPPERCASE label gets about 4-8% letter spacing.
- A paragraph has a line height of 120-150% of the font size (headings tighter, 110-130%; long body text looser, 150%) and a line of 45-75 characters. Short lines beat wide ones.
- Contrast: body text at least 4.5:1 against its background, large text and icons at least 3:1. Light grey text on white fails; check before you keep it.
- Pure white (`#ffffff`) only on a subtle dark background (`#111`-`#1a1a1a`, never pure black), pure black only on a subtle light one (`#fafafa`-`#f5f5f5`, never pure white). Otherwise use a near-white or near-black.
- Left-align paragraphs. Center only a short headline or a single line. Use at most two typefaces and a few weights (regular, medium, bold).
- Never put text on a busy picture without a scrim or a solid area under it.

## Color system

- Build each family as a sequence of steps from light to dark (for example 50, 100, 200 ... 900), and pick from the steps instead of a new hex each time.
- **Neutral**: for backgrounds, surfaces, borders and text (primary, secondary, tertiary, disabled). Most of every screen is neutral.
- **Accent**: one brand color for the primary action, the selected state and links. Used sparingly it shows what matters (the Von Restorff effect: the one thing that differs is remembered).
- **Semantic**: positive (green), negative (red), warning (amber), and info if needed, each with a strong step for text and icon and a pale step for its background.
  The accent may serve as positive when the brand is green.
- Color is never the only signal: pair it with an icon or a word.
- Roughly 60% neutral surface, 30% secondary/neutral text and structure, 10% accent.

## Subtlety for what is not important

Anything that is not the Obvious or the main Possible recedes: lighter weight, secondary or tertiary text color, smaller size, thinner or paler borders, no shadow.
Dividers are 1px in a pale neutral; shadows are soft and low. If everything is loud nothing is.

## Fewer boxes: do not lean on cards

A card is a container with a subtle background and a border. Do not wrap everything in one, and never put a card inside a card.

- Group with space first (proximity), then with a heading, then with a divider line, and only then with a surface.
- A card earns its place when it is a unit the person picks, opens or moves (a product, a post, a plan). A section of a page is not a card.
- When a card is used, keep one level: if something inside needs separating, use space or a divider.

## Tables and long lists

- **Paginate tables** (or load more / scroll within a fixed area): about 10-25 rows a page, with the page controls, the total count and the rows-per-page choice under the table. Never draw a table of 100 rows.
- Header row differs from body rows (smaller, uppercase or medium weight, secondary color). Align text left, numbers right (tabular figures), keep the row height the same (40-56px).
- Show a column's sort state, and put the row's actions at its end, subtle until the row is hovered.
- On mobile, a table becomes a list of rows with the key value and one line of detail.

## Laws of UX to apply

These are the laws from lawsofux.com that decide how a static screen is laid out.

- **Jakob's Law**: people expect your screen to work like the ones they know. Logo top left, navigation on top or left, search in a familiar place, a cart or avatar top right. Do not invent a pattern for a standard thing.
- **Hick's Law**: the more choices, the longer the decision. One primary action per screen, at most 5-7 items in a menu or tab bar, and break a long form into steps.
- **Fitts's Law**: a target is easier to hit when it is large and close. Buttons at least 44x44px on mobile and 32px high on web (40 is comfortable), with 8px or more between targets; put the main action where the thumb or pointer already is, and make a full row tappable, not only its text.
- **Miller's Law and chunking**: people hold about 7 items at once. Group content into chunks of 3-5 related items with a heading each (a phone number is 3-3-4, not ten digits).
- **Law of Proximity**: things close together are read as related. The gap inside a group is smaller than the gap between groups (for example 8 inside, 24 between), and a label sits nearer to its own field than to the one above.
- **Law of Similarity**: things that look alike are read as the same kind and things that differ as different. All links one style, all primary buttons one style.
- **Law of Common Region and Uniform Connectedness**: a shared background, a border or a line makes a group. Use it for the few groups that need it (see: do not lean on cards).
- **Law of Prägnanz**: people read a complex shape as the simplest one. Keep the layout to simple rectangles on a clear grid, with few alignment axes.
- **Von Restorff Effect (isolation)**: the one element that differs is the one remembered. Style one thing (the primary button, a "Recommended" plan) differently and nothing else.
- **Serial Position Effect**: the first and the last item are remembered best. Put the most important navigation item or action first or last, the least important in the middle.
- **Aesthetic-Usability Effect**: a screen that looks good is felt to work better. Spend care on alignment, spacing and type; it buys patience for small flaws.
- **Tesler's Law (conservation of complexity)**: some complexity cannot go away. Take it off the person: sensible defaults, prefilled fields, one clear path.
- **Postel's Law**: be tolerant in what you accept. Draw the fields that accept more than one format, and show an error that tells how to fix it, in plain words, next to the field.
- **Doherty Threshold**: feedback within 400ms keeps people engaged. Draw the loading, skeleton and progress states, not only the finished screen.
- **Goal-Gradient Effect**: people try harder as they near a goal. Show progress in a multi-step flow (step 2 of 4) and a head start (a bar already part full).
- **Peak-End Rule**: people judge an experience by its peak and its end. Give the last screen (done, success, receipt) real care, and make it say what happens next.
- **Zeigarnik Effect and Pareto Principle**: unfinished tasks are remembered, and most use goes to a few features. Show the person where they left off, and give the few features that matter most the space.
- **Occam's Razor and Choice Overload**: of two designs that do the same, take the simpler. Remove an element when nothing breaks without it.
- **Cognitive Load**: every extra element, color, font and decision costs attention. Fewer colors, fewer sizes, shorter copy, plainer words.
- **Mental Model**: people bring a picture of how it works. Name things the way they would (Cart, Sign in), not the way the system does.
- **Flow and Paradox of the Active User**: people do not read manuals, they start clicking. Put the next step in front of them, and the help next to the task, not in a separate screen.

## Every screen includes

- A clear primary action and a way back or out.
- The states that exist, as separate frames when they matter: empty (say what to do), loading, error, filled, long text (one that wraps), and one with no picture.
- Real copy, not "Lorem ipsum": a short verb for a button ("Save changes", not "Submit"), a title in sentence case, numbers and names that look real.
- A frame size that matches the target (web 1440x900 or 1280x800, tablet 834x1194, mobile 390x844) and auto layout inside it, so the screen resizes.

## Check before you say done

Export the screen (`GET /export/<id>`) and look at it. Squint: is the Obvious the first thing you see? Is there one primary action? Are all gaps multiples of 4 and the same for like things?
Is every text readable (size, contrast)? Is anything in a card that did not need one? Would a table longer than a page be paginated? Fix what is off, then answer.
