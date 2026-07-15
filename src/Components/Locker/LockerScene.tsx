import { Suspense } from "react";
import { Canvas } from "@react-three/fiber";
import {
  OrbitControls,
  ContactShadows,
  Environment,
  Lightformer,
} from "@react-three/drei";
import BikeModel from "./bikeModel";
import type { Livery } from "./liveries";

/**
 * The premium 3D stage: studio lighting, a procedural HDRI (Lightformers, so it
 * works fully offline), soft contact shadows and orbit/turntable controls.
 * Everything here is model-agnostic — swapping the bike mesh keeps the stage.
 */
export default function LockerScene({
  livery,
  number,
  autoRotate,
}: {
  livery: Livery;
  number: number;
  autoRotate: boolean;
}) {
  return (
    <Canvas
      shadows
      dpr={[1, 2]}
      gl={{ antialias: true, alpha: true, preserveDrawingBuffer: true }}
      camera={{ position: [2.3, 1.25, 2.7], fov: 34, near: 0.1, far: 100 }}
    >
      <Suspense fallback={null}>
        {/* Key / fill / rim lighting */}
        <hemisphereLight args={["#dfe7ef", "#0c0e12", 0.35]} />
        <directionalLight
          position={[3.2, 4.5, 2.5]}
          intensity={2.1}
          castShadow
          shadow-mapSize={[2048, 2048]}
          shadow-bias={-0.0002}
        >
          <orthographicCamera
            attach="shadow-camera"
            args={[-3, 3, 3, -3, 0.1, 20]}
          />
        </directionalLight>
        <directionalLight position={[-3, 2.5, -2]} intensity={0.7} color="#9ccfec" />
        <spotLight
          position={[-2, 3.5, 3]}
          angle={0.5}
          penumbra={0.8}
          intensity={1.2}
          color="#ffffff"
        />

        {/* Procedural environment for PBR reflections (no external HDR file). */}
        <Environment resolution={256}>
          <group rotation={[0, 0, 0]}>
            <Lightformer
              intensity={2}
              position={[0, 4, -3]}
              scale={[8, 3, 1]}
              color="#ffffff"
            />
            <Lightformer
              intensity={1.2}
              position={[4, 2, 2]}
              scale={[4, 4, 1]}
              color="#bcdcf2"
            />
            <Lightformer
              intensity={1}
              position={[-4, 1, 2]}
              scale={[4, 4, 1]}
              color="#7fa8c9"
            />
          </group>
        </Environment>

        <BikeModel livery={livery} number={number} />

        <ContactShadows
          position={[0, 0, 0]}
          opacity={0.55}
          scale={6}
          blur={2.4}
          far={2}
          resolution={1024}
          color="#000000"
        />

        <OrbitControls
          makeDefault
          enablePan={false}
          autoRotate={autoRotate}
          autoRotateSpeed={0.9}
          minDistance={1.8}
          maxDistance={6}
          minPolarAngle={0.15}
          maxPolarAngle={Math.PI / 2 - 0.02}
          target={[0, 0.55, 0]}
        />
      </Suspense>
    </Canvas>
  );
}
