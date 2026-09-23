import { expect, test } from "bun:test";
import { LOADING_MARK_GLYPH, LoadingMark } from "../src/Components/Shell/LoadingMark";

test("the page loading mark announces progress and uses the MXBsecure m", () => {
  const mark = LoadingMark({ label: "Opening mod" });
  const props = mark.props as {
    role: string;
    "aria-label": string;
    children: { props: { className: string; children: string } };
  };

  expect(props.role).toBe("status");
  expect(props["aria-label"]).toBe("Opening mod");
  expect(props.children.props.children).toBe(LOADING_MARK_GLYPH);
  expect(props.children.props.className).toContain("font-cond");
  expect(props.children.props.className).toContain("animate-pulse");
  expect(props.children.props.className).toContain("motion-reduce:animate-none");
});
