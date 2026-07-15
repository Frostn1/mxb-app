import { useMemo } from "react";
import * as THREE from "three";
import type { Livery } from "./liveries";
import { makeLiveryTexture } from "./liveries";

/**
 * A stylised motocross bike built from primitives. It is intentionally a
 * *generic* mesh: real per-bike models (converted from community FBX → glTF)
 * will replace this via a `<primitive object={gltf.scene} />` swap, while the
 * livery/material pipeline below stays identical. Bodywork panels carry the
 * livery; frame/engine/wheels are shared PBR materials.
 *
 * Orientation: bike faces +X, wheels roll about Z, ground plane at y = 0.
 */
export default function BikeModel({
  livery,
  number,
}: {
  livery: Livery;
  number: number;
}) {
  const texture = useMemo(
    () => makeLiveryTexture(livery, number),
    [livery, number],
  );
  // Dispose the previous canvas texture when the livery changes.
  useMemo(() => () => texture.dispose(), [texture]);

  const R = 0.4; // wheel radius

  return (
    <group position={[0, 0, 0]}>
      {/* Wheels */}
      <Wheel position={[0.82, R, 0]} radius={R} />
      <Wheel position={[-0.74, R, 0]} radius={R} />

      {/* Engine block */}
      <mesh position={[0.02, 0.5, 0]} castShadow>
        <boxGeometry args={[0.5, 0.44, 0.34]} />
        <Metal color="#3a3d42" rough={0.45} />
      </mesh>
      {/* Engine cases / cylinder head detail */}
      <mesh position={[0.06, 0.74, 0]} castShadow>
        <boxGeometry args={[0.34, 0.16, 0.3]} />
        <Metal color="#54585f" rough={0.35} />
      </mesh>

      {/* Frame spars */}
      <mesh position={[-0.05, 0.66, 0.13]} rotation={[0, 0, 0.5]} castShadow>
        <boxGeometry args={[0.7, 0.05, 0.04]} />
        <Metal color="#c9ccd2" rough={0.3} />
      </mesh>
      <mesh position={[-0.05, 0.66, -0.13]} rotation={[0, 0, 0.5]} castShadow>
        <boxGeometry args={[0.7, 0.05, 0.04]} />
        <Metal color="#c9ccd2" rough={0.3} />
      </mesh>

      {/* Fuel tank + main shroud — BODYWORK (livery) */}
      <mesh position={[0.2, 0.84, 0]} rotation={[0, 0, -0.08]} castShadow>
        <boxGeometry args={[0.6, 0.3, 0.44]} />
        <Body texture={texture} />
      </mesh>
      {/* Side number plates — BODYWORK (livery) */}
      <mesh position={[0.16, 0.62, 0.235]} rotation={[0.06, 0, 0.28]} castShadow>
        <boxGeometry args={[0.42, 0.36, 0.02]} />
        <Body texture={texture} />
      </mesh>
      <mesh position={[0.16, 0.62, -0.235]} rotation={[-0.06, 0, 0.28]} castShadow>
        <boxGeometry args={[0.42, 0.36, 0.02]} />
        <Body texture={texture} />
      </mesh>

      {/* Front fender — BODYWORK (base colour, clearcoat) */}
      <mesh position={[0.9, 0.72, 0]} rotation={[0, 0, -0.5]} castShadow>
        <boxGeometry args={[0.4, 0.03, 0.3]} />
        <Body color={livery.base} />
      </mesh>
      {/* Rear fender — BODYWORK */}
      <mesh position={[-0.7, 0.92, 0]} rotation={[0, 0, 0.18]} castShadow>
        <boxGeometry args={[0.5, 0.03, 0.32]} />
        <Body color={livery.base} />
      </mesh>

      {/* Seat */}
      <mesh position={[-0.34, 0.92, 0]} rotation={[0, 0, 0.05]} castShadow>
        <boxGeometry args={[0.56, 0.1, 0.28]} />
        <Plastic color="#141519" />
      </mesh>

      {/* Front forks */}
      <Fork position={[0.86, 0.68, 0.11]} />
      <Fork position={[0.86, 0.68, -0.11]} />
      {/* Triple clamp + bars (laid across Z) */}
      <mesh position={[0.8, 1.02, 0]} rotation={[Math.PI / 2, 0, 0]} castShadow>
        <cylinderGeometry args={[0.018, 0.018, 0.6, 16]} />
        <Metal color="#22242a" rough={0.4} />
      </mesh>

      {/* Swingarm to rear wheel */}
      <mesh position={[-0.4, 0.48, 0.1]} rotation={[0, 0, 0.12]} castShadow>
        <boxGeometry args={[0.5, 0.05, 0.03]} />
        <Metal color="#b9bcc2" rough={0.3} />
      </mesh>
      <mesh position={[-0.4, 0.48, -0.1]} rotation={[0, 0, 0.12]} castShadow>
        <boxGeometry args={[0.5, 0.05, 0.03]} />
        <Metal color="#b9bcc2" rough={0.3} />
      </mesh>

      {/* Exhaust */}
      <mesh position={[-0.15, 0.66, -0.2]} rotation={[0, 0, 1.2]} castShadow>
        <cylinderGeometry args={[0.05, 0.06, 0.5, 18]} />
        <Metal color="#8f9298" rough={0.25} />
      </mesh>
      <mesh position={[-0.55, 0.72, -0.22]} rotation={[0, 0, 1.5]} castShadow>
        <cylinderGeometry args={[0.06, 0.055, 0.45, 18]} />
        <Metal color="#c7c9ce" rough={0.2} />
      </mesh>
    </group>
  );
}

function Wheel({
  position,
  radius,
}: {
  position: [number, number, number];
  radius: number;
}) {
  return (
    <group position={position}>
      {/* Tire */}
      <mesh castShadow rotation={[Math.PI / 2, 0, 0]}>
        <torusGeometry args={[radius - 0.08, 0.085, 20, 48]} />
        <meshStandardMaterial color="#0c0c0e" roughness={0.92} metalness={0} />
      </mesh>
      {/* Rim */}
      <mesh rotation={[Math.PI / 2, 0, 0]} castShadow>
        <cylinderGeometry args={[radius - 0.13, radius - 0.13, 0.06, 40]} />
        <meshStandardMaterial color="#d6d8dc" roughness={0.28} metalness={0.9} />
      </mesh>
      {/* Hub */}
      <mesh rotation={[Math.PI / 2, 0, 0]}>
        <cylinderGeometry args={[0.07, 0.07, 0.12, 24]} />
        <meshStandardMaterial color="#4a4d53" roughness={0.4} metalness={0.8} />
      </mesh>
    </group>
  );
}

function Fork({ position }: { position: [number, number, number] }) {
  return (
    <mesh position={position} rotation={[0, 0, -0.5]} castShadow>
      <cylinderGeometry args={[0.026, 0.026, 0.62, 16]} />
      <meshStandardMaterial color="#e7e9ec" roughness={0.18} metalness={1} />
    </mesh>
  );
}

/** Painted bodywork: physical clearcoat over a livery texture or a base colour. */
function Body({
  texture,
  color,
}: {
  texture?: THREE.Texture;
  color?: string;
}) {
  return (
    <meshPhysicalMaterial
      map={texture}
      color={texture ? "#ffffff" : (color ?? "#cccccc")}
      roughness={0.32}
      metalness={0.0}
      clearcoat={0.7}
      clearcoatRoughness={0.28}
      envMapIntensity={1.1}
    />
  );
}

function Metal({ color, rough = 0.35 }: { color: string; rough?: number }) {
  return (
    <meshStandardMaterial color={color} roughness={rough} metalness={0.85} />
  );
}

function Plastic({ color }: { color: string }) {
  return <meshStandardMaterial color={color} roughness={0.6} metalness={0} />;
}
