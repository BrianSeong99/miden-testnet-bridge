import React from "react";
import { Composition } from "remotion";
import { BridgeWalkthrough } from "./Walkthrough";
import {
  TerminalWalkthrough,
  terminalWalkthroughDurationFrames,
} from "./TerminalWalkthrough";

export const Root: React.FC = () => {
  return (
    <>
      <Composition
        id="BridgeWalkthrough"
        component={BridgeWalkthrough}
        durationInFrames={2160}
        fps={30}
        width={1920}
        height={1080}
      />
      <Composition
        id="TerminalWalkthrough"
        component={TerminalWalkthrough}
        durationInFrames={terminalWalkthroughDurationFrames}
        fps={30}
        width={1920}
        height={1080}
      />
    </>
  );
};
