import { Composition } from "remotion";
import { JetDemo } from "./JetDemo";

export const RemotionRoot: React.FC = () => {
  return (
    <Composition
      id="JetDemo"
      component={JetDemo}
      durationInFrames={30 * 30}
      fps={30}
      width={1200}
      height={800}
    />
  );
};
