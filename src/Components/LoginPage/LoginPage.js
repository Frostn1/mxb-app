import { useTheme } from "@mui/material";
import "./LoginPage.scss";

const LoginPage = (props) => {
  const theme = useTheme();

  return (
    <div id={"login-page"}>
      <div
        className={"background"}
        style={{
          backgroundImage: `radial-gradient(${theme.palette.text.disabled} 1px, transparent 1px)`,
        }}
      />

      <div
        className={"author"}
        style={{
          background: `linear-gradient(${theme.palette.text.primary} 0 0) right / 3px 50% no-repeat`,
        }}
      >
        MXBMM
      </div>
    </div>
  );
};

LoginPage.propTypes = {};

export default LoginPage;
